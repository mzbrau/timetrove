use super::client::{self, CopilotCredentials};
use crate::db::Db;
use crate::domain::{CopilotUsageHistoryPoint, CopilotUsageSummary};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

static POLL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static POLL_QUEUED: AtomicBool = AtomicBool::new(false);

pub fn credentials_from_db(db: &Db) -> Result<Option<CopilotCredentials>, String> {
    let settings = db.get_settings()?;
    if !settings.copilot_enabled {
        return Ok(None);
    }
    if settings.copilot_api_token.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some(CopilotCredentials {
        github_host: settings.copilot_github_host,
        api_token: settings.copilot_api_token,
        username: settings.copilot_username,
        monthly_limit_override: settings.copilot_monthly_limit_override,
    }))
}

/// Restore top-level quota fields from the primary pool (API-native values).
/// Used when an older cache may have baked an override into the summary.
fn restore_from_primary_pool(summary: &mut CopilotUsageSummary) {
    let Some(pool) = summary
        .pools
        .iter()
        .find(|p| p.id == "premium_interactions")
        .cloned()
        .or_else(|| summary.pools.first().cloned())
    else {
        return;
    };
    summary.entitlement = pool.entitlement;
    summary.used = pool.used;
    summary.remaining = pool.remaining;
    summary.percent_used = pool.percent_used;
    summary.unlimited = pool.unlimited;
    summary.overage_count = pool.overage_count;
    summary.overage_permitted = pool.overage_permitted;
}

pub fn load_cached_summary(db: &Db) -> Result<Option<CopilotUsageSummary>, String> {
    let settings = db.get_settings()?;
    if !settings.copilot_enabled {
        return Ok(None);
    }
    let mut summary = match db.get_copilot_usage_cache()? {
        Some(json) => serde_json::from_str::<CopilotUsageSummary>(&json).ok(),
        None => None,
    };
    if let Some(ref mut s) = summary {
        s.last_synced_at = settings.copilot_last_synced_at.clone();
        s.last_error = settings.copilot_last_error.clone();
        // Older caches may have baked override into top-level fields; pools stay API-native.
        if s.limit_source == "override" && !s.pools.is_empty() {
            restore_from_primary_pool(s);
        }
        *s = client::apply_limit_override(s.clone(), settings.copilot_monthly_limit_override);
    } else if settings.copilot_last_error.is_some() {
        // Surface error state even without a successful cache.
        summary = Some(CopilotUsageSummary {
            plan: String::new(),
            username: settings.copilot_username,
            unit: "requests".into(),
            entitlement: 0.0,
            used: 0.0,
            remaining: 0.0,
            percent_used: 0.0,
            unlimited: false,
            overage_count: 0.0,
            overage_permitted: false,
            reset_date: None,
            pools: vec![],
            limit_source: "unknown".into(),
            fetched_at: None,
            last_synced_at: settings.copilot_last_synced_at,
            last_error: settings.copilot_last_error,
        });
    }
    Ok(summary)
}

pub fn persist_success(
    db: &Db,
    mut summary: CopilotUsageSummary,
    raw_json: &str,
) -> Result<CopilotUsageSummary, String> {
    let now = Utc::now();
    summary.fetched_at = Some(now.to_rfc3339());
    summary.last_synced_at = Some(now.to_rfc3339());
    summary.last_error = None;

    if !summary.username.trim().is_empty() {
        // Persist discovered username when settings left it blank (atomic, no full rewrite).
        db.set_copilot_username_if_blank(&summary.username)?;
    }

    // Cache API-native summary; override is applied at read time.
    let cache = serde_json::to_string(&summary).map_err(|e| e.to_string())?;
    db.set_copilot_usage_cache(&cache)?;
    db.set_copilot_last_error(None)?;
    db.set_copilot_last_synced_at(Some(now))?;

    let point = CopilotUsageHistoryPoint {
        id: Uuid::new_v4().to_string(),
        fetched_at: now,
        plan: summary.plan.clone(),
        entitlement: summary.entitlement,
        used: summary.used,
        remaining: summary.remaining,
        percent_used: summary.percent_used,
        unit: summary.unit.clone(),
    };
    db.insert_copilot_usage_snapshot(&point, raw_json)?;
    Ok(summary)
}

pub fn persist_error(db: &Db, err: &str) -> Result<(), String> {
    db.set_copilot_last_error(Some(err))
}

fn config_still_matches(db: &Db, started: &CopilotCredentials) -> Result<bool, String> {
    match credentials_from_db(db)? {
        Some(current) => Ok(current.config_matches(started)),
        None => Ok(false),
    }
}

/// Apply current settings override for display (poll return / live paths).
fn with_current_override(db: &Db, summary: CopilotUsageSummary) -> Result<CopilotUsageSummary, String> {
    let settings = db.get_settings()?;
    Ok(client::apply_limit_override(
        summary,
        settings.copilot_monthly_limit_override,
    ))
}

/// Fetch and persist; returns the updated summary (with current override applied for display).
pub fn poll_once(db: &Db) -> Result<Option<CopilotUsageSummary>, String> {
    let Some(creds) = credentials_from_db(db)? else {
        return Ok(None);
    };
    match client::fetch_usage(&creds) {
        Ok((summary, raw)) => {
            if !config_still_matches(db, &creds)? {
                // Credentials changed mid-flight — discard; a queued poll will re-fetch.
                return Ok(None);
            }
            let saved = persist_success(db, summary, &raw)?;
            Ok(Some(with_current_override(db, saved)?))
        }
        Err(e) => {
            if !config_still_matches(db, &creds)? {
                return Ok(None);
            }
            let _ = persist_error(db, &e);
            Err(e)
        }
    }
}

pub fn spawn_poll(db: Arc<Db>, on_done: impl FnOnce() + Send + 'static) {
    if POLL_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        POLL_QUEUED.store(true, Ordering::SeqCst);
        return;
    }
    std::thread::spawn(move || {
        'poll: loop {
            let _ = poll_once(&db);
            if POLL_QUEUED.swap(false, Ordering::SeqCst) {
                // Run again with fresh credentials from the queue request.
                continue;
            }
            POLL_IN_PROGRESS.store(false, Ordering::SeqCst);
            // Late queue between last swap(false) and clearing in-progress: reclaim and continue.
            if POLL_QUEUED.load(Ordering::SeqCst)
                && POLL_IN_PROGRESS
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                if POLL_QUEUED.swap(false, Ordering::SeqCst) {
                    continue 'poll;
                }
                POLL_IN_PROGRESS.store(false, Ordering::SeqCst);
            }
            break;
        }
        on_done();
    });
}

pub fn poll_interval_secs(db: &Db) -> u64 {
    db.get_settings()
        .map(|s| s.copilot_poll_interval_mins.clamp(5, 120) as u64 * 60)
        .unwrap_or(30 * 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::CopilotQuotaPool;

    fn unlimited_summary() -> CopilotUsageSummary {
        CopilotUsageSummary {
            plan: "business".into(),
            username: "me".into(),
            unit: "requests".into(),
            entitlement: 0.0,
            used: 12.0,
            remaining: 0.0,
            percent_used: 0.0,
            unlimited: true,
            overage_count: 0.0,
            overage_permitted: false,
            reset_date: None,
            pools: vec![CopilotQuotaPool {
                id: "premium_interactions".into(),
                entitlement: 0.0,
                remaining: 0.0,
                used: 12.0,
                percent_remaining: 100.0,
                percent_used: 0.0,
                unlimited: true,
                overage_count: 0.0,
                overage_permitted: false,
            }],
            limit_source: "unlimited".into(),
            fetched_at: None,
            last_synced_at: None,
            last_error: None,
        }
    }

    #[test]
    fn restore_then_reapply_override_changes_limit() {
        // Simulate an older cache that baked override=300 into top-level fields.
        let mut cached = unlimited_summary();
        cached.entitlement = 300.0;
        cached.remaining = 288.0;
        cached.percent_used = 4.0;
        cached.unlimited = false;
        cached.limit_source = "override".into();

        restore_from_primary_pool(&mut cached);
        assert!(cached.unlimited);
        assert_eq!(cached.entitlement, 0.0);

        let with_600 = client::apply_limit_override(cached.clone(), Some(600.0));
        assert_eq!(with_600.limit_source, "override");
        assert_eq!(with_600.entitlement, 600.0);
        assert!((with_600.percent_used - 2.0).abs() < 0.01);

        let cleared = client::apply_limit_override(cached, None);
        assert_eq!(cleared.limit_source, "unlimited");
        assert!(cleared.unlimited);
    }

    #[test]
    fn credentials_config_matches_ignores_username() {
        let a = CopilotCredentials {
            github_host: "github.com".into(),
            api_token: "tok".into(),
            username: "".into(),
            monthly_limit_override: Some(100.0),
        };
        let b = CopilotCredentials {
            github_host: "github.com".into(),
            api_token: "tok".into(),
            username: "octocat".into(),
            monthly_limit_override: Some(100.0),
        };
        assert!(a.config_matches(&b));
        let c = CopilotCredentials {
            github_host: "github.com".into(),
            api_token: "other".into(),
            username: "octocat".into(),
            monthly_limit_override: Some(100.0),
        };
        assert!(!a.config_matches(&c));
    }
}
