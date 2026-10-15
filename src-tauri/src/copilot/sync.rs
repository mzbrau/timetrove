use super::client::{self, CopilotCredentials};
use crate::db::Db;
use crate::domain::{CopilotUsageHistoryPoint, CopilotUsageSummary};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use uuid::Uuid;

static POLL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

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
        // Re-apply override in case settings changed since last poll.
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

    if summary.username.trim().is_empty() {
        // keep as-is
    } else {
        // Persist discovered username when settings left it blank.
        let mut settings = db.get_settings()?;
        if settings.copilot_username.trim().is_empty() {
            settings.copilot_username = summary.username.clone();
            db.update_settings(&settings)?;
        }
    }

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

/// Fetch and persist; returns the updated summary.
pub fn poll_once(db: &Db) -> Result<Option<CopilotUsageSummary>, String> {
    let Some(creds) = credentials_from_db(db)? else {
        return Ok(None);
    };
    match client::fetch_usage(&creds) {
        Ok((summary, raw)) => {
            let saved = persist_success(db, summary, &raw)?;
            Ok(Some(saved))
        }
        Err(e) => {
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
        return;
    }
    std::thread::spawn(move || {
        let _ = poll_once(&db);
        POLL_IN_PROGRESS.store(false, Ordering::SeqCst);
        on_done();
    });
}

pub fn poll_interval_secs(db: &Db) -> u64 {
    db.get_settings()
        .map(|s| s.copilot_poll_interval_mins.clamp(5, 120) as u64 * 60)
        .unwrap_or(30 * 60)
}
