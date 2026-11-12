use super::client;
use super::http::GithubHttpConfig;
use crate::db::Db;
use crate::domain::{
    GithubActivityDigest, GithubPrCheck, GithubPrDetail, GithubPrReview, GithubPrSummary,
    GithubPrThread, GithubPrTimelineItem, GithubPrTimelineKind,
};
use chrono::Utc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use tauri::AppHandle;

static POLL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
static POLL_QUEUED: AtomicBool = AtomicBool::new(false);
/// Serializes fetch+persist so manual refresh and background polls cannot overwrite out of order.
static POLL_MUTEX: Mutex<()> = Mutex::new(());

pub fn credentials_from_db(db: &Db) -> Result<Option<(GithubHttpConfig, String)>, String> {
    let settings = db.get_settings()?;
    if !settings.github_activity_enabled {
        return Ok(None);
    }
    if settings.copilot_api_token.trim().is_empty() {
        return Ok(None);
    }
    Ok(Some((
        GithubHttpConfig {
            github_host: settings.copilot_github_host,
            api_token: settings.copilot_api_token,
            proxy_url: settings.copilot_proxy_url,
        },
        settings.copilot_username,
    )))
}

fn config_matches(a: &GithubHttpConfig, b: &GithubHttpConfig) -> bool {
    a.github_host.trim().eq_ignore_ascii_case(b.github_host.trim())
        && a.api_token.trim() == b.api_token.trim()
        && a.proxy_url.trim() == b.proxy_url.trim()
}

pub fn load_cached_digest(db: &Db) -> Result<Option<GithubActivityDigest>, String> {
    let settings = db.get_settings()?;
    if !settings.github_activity_enabled {
        return Ok(None);
    }
    let mut digest = match db.get_github_activity_cache()? {
        Some(json) => serde_json::from_str::<GithubActivityDigest>(&json).ok(),
        None => None,
    };
    if let Some(ref mut d) = digest {
        d.last_synced_at = settings.github_activity_last_synced_at.clone();
        d.last_error = settings.github_activity_last_error.clone();
    } else if settings.github_activity_last_error.is_some() {
        digest = Some(GithubActivityDigest {
            authored_prs: vec![],
            review_prs: vec![],
            recent_commits: vec![],
            username: settings.copilot_username,
            fetched_at: None,
            last_synced_at: settings.github_activity_last_synced_at,
            last_error: settings.github_activity_last_error,
        });
    }
    Ok(digest)
}

/// Local-only PR detail for demo mode (no network). Built from the cached digest summary.
pub fn demo_pr_detail(
    db: &Db,
    repo_full_name: &str,
    number: i32,
) -> Result<GithubPrDetail, String> {
    let digest = load_cached_digest(db)?
        .ok_or_else(|| "No cached GitHub activity in demo mode".to_string())?;
    let pr = digest
        .authored_prs
        .iter()
        .chain(digest.review_prs.iter())
        .find(|p| p.repo_full_name == repo_full_name && p.number == number)
        .cloned()
        .ok_or_else(|| {
            format!("Demo PR {repo_full_name}#{number} not found in local cache")
        })?;
    Ok(detail_from_summary(&pr))
}

fn detail_from_summary(pr: &GithubPrSummary) -> GithubPrDetail {
    let check_state = if pr.check_status.is_empty() {
        "PENDING".to_string()
    } else {
        pr.check_status.clone()
    };
    let checks = vec![GithubPrCheck {
        name: "CI".into(),
        state: check_state,
        description: "Demo check status from cached summary".into(),
        url: pr.url.clone(),
    }];

    let mut reviews = Vec::new();
    let decision = pr.review_decision.to_ascii_uppercase();
    if decision == "APPROVED" || decision == "CHANGES_REQUESTED" {
        reviews.push(GithubPrReview {
            author: "demo-reviewer".into(),
            state: decision,
            submitted_at: pr.created_at.clone(),
        });
    }

    let open_threads = (0..pr.unresolved_comment_count.min(3))
        .map(|i| GithubPrThread {
            path: format!("src/demo{i}.rs"),
            line: Some(10 + i as i32),
            author: "demo-reviewer".into(),
            body: format!("Demo unresolved comment {}", i + 1),
            created_at: pr.created_at.clone(),
        })
        .collect();

    let mut timeline = vec![GithubPrTimelineItem {
        kind: GithubPrTimelineKind::Opened,
        at: pr.created_at.clone(),
        actor: "demo".into(),
        summary: "Pull request opened".into(),
        url: pr.url.clone(),
    }];
    if pr.commit_count > 0 {
        let short = if pr.head_oid.len() >= 7 {
            pr.head_oid[..7].to_string()
        } else if pr.head_oid.is_empty() {
            "demo000".into()
        } else {
            pr.head_oid.clone()
        };
        timeline.push(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Commit,
            at: pr.created_at.clone(),
            actor: String::new(),
            summary: format!("{short} Latest demo commit ({})", pr.commit_count),
            url: pr.url.clone(),
        });
    }
    if !pr.reviews_summary.is_empty() && pr.reviews_summary != "No reviews" {
        timeline.push(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Review,
            at: pr.created_at.clone(),
            actor: "demo-reviewer".into(),
            summary: format!("Review: {}", pr.reviews_summary),
            url: pr.url.clone(),
        });
    }
    timeline.reverse();

    GithubPrDetail {
        number: pr.number,
        repo_full_name: pr.repo_full_name.clone(),
        title: pr.title.clone(),
        url: pr.url.clone(),
        checks,
        reviews,
        pending_reviewers: if pr.needs_my_review {
            vec!["you".into()]
        } else {
            vec![]
        },
        open_threads,
        timeline,
        fetched_at: Utc::now().to_rfc3339(),
    }
}

pub fn persist_success(db: &Db, mut digest: GithubActivityDigest) -> Result<GithubActivityDigest, String> {
    let now = Utc::now();
    digest.fetched_at = Some(now.to_rfc3339());
    digest.last_synced_at = Some(now.to_rfc3339());
    digest.last_error = None;

    if !digest.username.trim().is_empty() {
        db.set_copilot_username_if_blank(&digest.username)?;
    }

    let cache = serde_json::to_string(&digest).map_err(|e| e.to_string())?;
    db.set_github_activity_cache(&cache)?;
    db.set_github_activity_last_error(None)?;
    db.set_github_activity_last_synced_at(Some(now))?;
    Ok(digest)
}

pub fn persist_error(db: &Db, err: &str) -> Result<(), String> {
    db.set_github_activity_last_error(Some(err))
}

fn config_still_matches(db: &Db, started: &GithubHttpConfig) -> Result<bool, String> {
    match credentials_from_db(db)? {
        Some((current, _)) => Ok(config_matches(&current, started)),
        None => Ok(false),
    }
}

/// Fetch, persist, and advance GitHub notifications under one mutex.
/// Manual refresh and background polls share this path so snapshots cannot race.
pub fn poll_once(db: &Db, app: Option<&AppHandle>) -> Result<Option<GithubActivityDigest>, String> {
    let _guard = POLL_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if crate::demo::enabled() {
        let digest = load_cached_digest(db)?;
        if let (Some(app), Some(ref d)) = (app, &digest) {
            crate::notify::check_github_activity(app, db, d);
        }
        return Ok(digest);
    }
    let Some((creds, username)) = credentials_from_db(db)? else {
        return Ok(None);
    };
    match client::fetch_activity(&creds, &username) {
        Ok(digest) => {
            if !config_still_matches(db, &creds)? {
                return Ok(None);
            }
            let digest = persist_success(db, digest)?;
            if let Some(app) = app {
                crate::notify::check_github_activity(app, db, &digest);
            }
            Ok(Some(digest))
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

pub fn spawn_poll(db: Arc<Db>, app: AppHandle, on_done: impl FnOnce() + Send + 'static) {
    if POLL_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        POLL_QUEUED.store(true, Ordering::SeqCst);
        return;
    }
    std::thread::spawn(move || {
        'poll: loop {
            let _ = poll_once(&db, Some(&app));
            if POLL_QUEUED.swap(false, Ordering::SeqCst) {
                continue;
            }
            POLL_IN_PROGRESS.store(false, Ordering::SeqCst);
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
        .map(|s| s.github_activity_poll_interval_mins.clamp(5, 120) as u64 * 60)
        .unwrap_or(30 * 60)
}
