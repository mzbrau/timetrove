use super::client::{add_worklog, JiraCredentials};
use super::MIN_WORKLOG_MS;
use crate::db::Db;
use chrono::{Duration, Utc};
use parking_lot::Mutex;
use std::sync::Arc;

static SYNC_LOCK: Mutex<()> = Mutex::new(());

/// Whole-second unreported ms ready to post (0 if under minimum or over-reported).
pub fn unreported_ms(total_ms: i64, reported_ms: i64) -> i64 {
    let raw = total_ms - reported_ms;
    if raw < MIN_WORKLOG_MS {
        return 0;
    }
    // Floor to whole seconds for the worklog API.
    (raw / 1000) * 1000
}

fn credentials_from_db(db: &Db) -> Result<Option<JiraCredentials>, String> {
    let settings = db.get_settings()?;
    if !settings.jira_enabled {
        return Ok(None);
    }
    if settings.jira_site_url.trim().is_empty()
        || settings.jira_email.trim().is_empty()
        || settings.jira_api_token.trim().is_empty()
    {
        return Ok(None);
    }
    Ok(Some(JiraCredentials {
        site_url: settings.jira_site_url,
        email: settings.jira_email,
        api_token: settings.jira_api_token,
    }))
}

/// Sync one task's unreported time to Jira. No-op when disabled or nothing due.
pub fn sync_task(db: &Db, task_id: &str) -> Result<(), String> {
    let _guard = SYNC_LOCK.lock();
    let Some(creds) = credentials_from_db(db)? else {
        return Ok(());
    };
    let task = match db.get_task(task_id)? {
        Some(t) => t,
        None => return Ok(()),
    };
    if task.jira_issue_key.is_empty() {
        return Ok(());
    }
    let now = Utc::now();
    let total = db.task_total_ms(task_id, now)?;
    let to_post = unreported_ms(total, task.jira_reported_ms);
    if to_post <= 0 {
        return Ok(());
    }
    let seconds = to_post / 1000;
    let started = now - Duration::milliseconds(to_post);
    match add_worklog(&creds, &task.jira_issue_key, seconds, started) {
        Ok(()) => {
            db.bump_jira_reported_ms(task_id, to_post)?;
            db.set_jira_last_error(None)?;
            db.set_jira_last_synced_at(Some(Utc::now()))?;
            Ok(())
        }
        Err(e) => {
            let _ = db.set_jira_last_error(Some(&e));
            Err(e)
        }
    }
}

/// Sync all tasks that have a Jira issue key and unreported time.
pub fn sync_all_due(db: &Db) -> Result<(), String> {
    let _guard = SYNC_LOCK.lock();
    let Some(creds) = credentials_from_db(db)? else {
        return Ok(());
    };
    let tasks = db.list_tasks_with_jira_key()?;
    let now = Utc::now();
    let mut last_err: Option<String> = None;
    let mut any_ok = false;
    for task in tasks {
        let total = match db.task_total_ms(&task.id, now) {
            Ok(ms) => ms,
            Err(e) => {
                last_err = Some(e);
                continue;
            }
        };
        let to_post = unreported_ms(total, task.jira_reported_ms);
        if to_post <= 0 {
            continue;
        }
        let seconds = to_post / 1000;
        let started = now - Duration::milliseconds(to_post);
        match add_worklog(&creds, &task.jira_issue_key, seconds, started) {
            Ok(()) => {
                if let Err(e) = db.bump_jira_reported_ms(&task.id, to_post) {
                    last_err = Some(e);
                    continue;
                }
                any_ok = true;
            }
            Err(e) => {
                last_err = Some(e);
            }
        }
    }
    if any_ok {
        let _ = db.set_jira_last_synced_at(Some(Utc::now()));
    }
    match last_err {
        Some(e) => {
            let _ = db.set_jira_last_error(Some(&e));
            Err(e)
        }
        None => {
            let _ = db.set_jira_last_error(None);
            Ok(())
        }
    }
}

pub fn spawn_flush_task(db: Arc<Db>, task_id: String) {
    std::thread::spawn(move || {
        let _ = sync_task(&db, &task_id);
    });
}

pub fn spawn_flush_all(db: Arc<Db>) {
    std::thread::spawn(move || {
        let _ = sync_all_due(&db);
    });
}
