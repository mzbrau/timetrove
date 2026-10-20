use super::{click_for_items, format_item_body, get_setting_string, set_setting_string, show};
use crate::db::Db;
use crate::domain::JiraIssueDetails;
use crate::jira::client::browse_url;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

const KEY_SNAPSHOT: &str = "jira_notify_snapshot";
pub const POLL_INTERVAL_SECS: u64 = 5 * 60;

static POLL_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueSnap {
    pub status: String,
    pub summary: String,
    pub browse_url: String,
    pub comment_total: u32,
    pub latest_comment_id: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraNotifySnapshot {
    pub issues: HashMap<String, JiraIssueSnap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JiraChangeCategory {
    pub title: String,
    pub labels: Vec<String>,
    pub urls: Vec<String>,
}

fn issue_label(key: &str, summary: &str) -> String {
    if summary.is_empty() {
        key.to_string()
    } else {
        format!("{key} — {summary}")
    }
}

pub fn snapshot_from_issues(issues: &[JiraIssueDetails], site_url: &str) -> JiraNotifySnapshot {
    let mut map = HashMap::new();
    for issue in issues {
        let browse = browse_url(site_url, &issue.issue_key).unwrap_or_default();
        map.insert(
            issue.issue_key.clone(),
            JiraIssueSnap {
                status: issue.status.clone(),
                summary: issue.summary.clone(),
                browse_url: browse,
                comment_total: issue.comment_total,
                latest_comment_id: issue.latest_comment_id.clone(),
            },
        );
    }
    JiraNotifySnapshot { issues: map }
}

pub fn diff_jira(prev: &JiraNotifySnapshot, next: &JiraNotifySnapshot) -> Vec<JiraChangeCategory> {
    let mut assigned: Vec<(String, String)> = Vec::new();
    let mut status: Vec<(String, String)> = Vec::new();
    let mut comments: Vec<(String, String)> = Vec::new();
    let mut left: Vec<(String, String)> = Vec::new();

    for (key, cur) in &next.issues {
        let label = issue_label(key, &cur.summary);
        match prev.issues.get(key) {
            None => assigned.push((label, cur.browse_url.clone())),
            Some(old) => {
                if old.status != cur.status {
                    status.push((label.clone(), cur.browse_url.clone()));
                }
                let comment_grew = cur.comment_total > old.comment_total
                    || (!cur.latest_comment_id.is_empty()
                        && cur.latest_comment_id != old.latest_comment_id);
                if comment_grew {
                    comments.push((label, cur.browse_url.clone()));
                }
            }
        }
    }

    for (key, old) in &prev.issues {
        if !next.issues.contains_key(key) {
            left.push((issue_label(key, &old.summary), old.browse_url.clone()));
        }
    }

    let mut out = Vec::new();
    let push = |out: &mut Vec<JiraChangeCategory>, title: &str, items: Vec<(String, String)>| {
        if items.is_empty() {
            return;
        }
        let (labels, urls): (Vec<_>, Vec<_>) = items.into_iter().unzip();
        out.push(JiraChangeCategory {
            title: title.into(),
            labels,
            urls,
        });
    };
    push(&mut out, "New Jira assignments", assigned);
    push(&mut out, "Jira status changed", status);
    push(&mut out, "New Jira comments", comments);
    push(&mut out, "Jira tickets left your queue", left);
    out
}

pub fn check_jira_assigned(app: &AppHandle, db: &Db, issues: &[JiraIssueDetails], site_url: &str) {
    let Ok(settings) = db.get_settings() else {
        return;
    };
    if !settings.notify_jira_assigned || !settings.jira_enabled {
        return;
    }

    let next = snapshot_from_issues(issues, site_url);
    let prev_json = get_setting_string(db, KEY_SNAPSHOT)
        .ok()
        .flatten()
        .unwrap_or_default();
    let baseline = prev_json.is_empty();
    let prev: JiraNotifySnapshot = if baseline {
        JiraNotifySnapshot::default()
    } else {
        serde_json::from_str(&prev_json).unwrap_or_default()
    };

    if !baseline {
        for cat in diff_jira(&prev, &next) {
            let body = format_item_body(&cat.labels);
            let action = click_for_items(&cat.urls, "jira");
            show(app, &cat.title, &body, action);
        }
    }

    if let Ok(json) = serde_json::to_string(&next) {
        let _ = set_setting_string(db, KEY_SNAPSHOT, &json);
    }
}

pub fn jira_poll_interval_secs() -> u64 {
    POLL_INTERVAL_SECS
}

/// Force-refresh assigned issues and emit notifications on change.
pub fn poll_jira_notify_once(app: &AppHandle, db: &Db) {
    let Ok(settings) = db.get_settings() else {
        return;
    };
    if !settings.notify_jira_assigned || !settings.jira_enabled {
        return;
    }
    if settings.jira_site_url.trim().is_empty()
        || settings.jira_email.trim().is_empty()
        || settings.jira_api_token.trim().is_empty()
    {
        return;
    }

    // Refresh shared cache (UI) and read issues with comment fingerprints.
    let _ = crate::jira::list_assigned(db, true);
    let Some((site_url, issues)) = crate::jira::assigned_issues_for_notify() else {
        if crate::demo::enabled() {
            let fixture = crate::demo::jira_fixture();
            check_jira_assigned(app, db, &fixture, &settings.jira_site_url);
            let _ = app.emit("jira-assigned-updated", ());
        }
        return;
    };
    check_jira_assigned(app, db, &issues, &site_url);
    let _ = app.emit("jira-assigned-updated", ());
}

pub fn spawn_jira_notify_poll(db: Arc<Db>, app: AppHandle) {
    if POLL_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return;
    }
    std::thread::spawn(move || {
        poll_jira_notify_once(&app, &db);
        POLL_IN_PROGRESS.store(false, Ordering::SeqCst);
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snap(status: &str, comments: u32, id: &str) -> JiraIssueSnap {
        JiraIssueSnap {
            status: status.into(),
            summary: "Title".into(),
            browse_url: "https://example.atlassian.net/browse/X".into(),
            comment_total: comments,
            latest_comment_id: id.into(),
        }
    }

    #[test]
    fn detects_new_assignment_and_left() {
        let mut prev = JiraNotifySnapshot::default();
        prev.issues.insert("A-1".into(), snap("To Do", 0, ""));
        let mut next = JiraNotifySnapshot::default();
        next.issues.insert("B-2".into(), snap("To Do", 0, ""));
        let cats = diff_jira(&prev, &next);
        assert!(cats.iter().any(|c| c.title.contains("New")));
        assert!(cats.iter().any(|c| c.title.contains("left")));
    }

    #[test]
    fn detects_status_and_comment() {
        let mut prev = JiraNotifySnapshot::default();
        prev.issues.insert("A-1".into(), snap("To Do", 1, "10"));
        let mut next = JiraNotifySnapshot::default();
        next.issues
            .insert("A-1".into(), snap("In Progress", 2, "11"));
        let cats = diff_jira(&prev, &next);
        assert_eq!(cats.len(), 2);
        assert!(cats.iter().any(|c| c.title.contains("status")));
        assert!(cats.iter().any(|c| c.title.contains("comments")));
    }
}
