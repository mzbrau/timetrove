//! In-memory cache of assigned open Jira issues (shared across webviews).

use crate::db::Db;
use crate::domain::{JiraAssignedIssuesResult, JiraDashboardIssue, JiraIssueDetails, Task};
use crate::jira::client::{
    browse_url, search_my_open_issues, ASSIGNED_ISSUES_MAX, JiraCredentials,
};
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::sync::Mutex;

const CACHE_TTL: Duration = Duration::minutes(5);

#[derive(Debug, Default)]
struct CacheInner {
    fetched_at: Option<DateTime<Utc>>,
    issues: Vec<JiraIssueDetails>,
    truncated: bool,
    site_url: String,
    /// Normalized email (trim + lowercase) for the credentials that filled the cache.
    account_id: String,
}

static CACHE: Mutex<CacheInner> = Mutex::new(CacheInner {
    fetched_at: None,
    issues: Vec::new(),
    truncated: false,
    site_url: String::new(),
    account_id: String::new(),
});

/// Normalize Jira account identity for cache keys.
pub fn normalize_account_id(email: &str) -> String {
    email.trim().to_lowercase()
}

fn cache_matches_identity(cache: &CacheInner, site_url: &str, account_id: &str) -> bool {
    !cache.site_url.is_empty()
        && !cache.account_id.is_empty()
        && cache.site_url == site_url
        && cache.account_id == account_id
}

/// TimeTrove booked ms for the current Jira key on a task.
pub fn timetrove_booked_ms(task: &Task) -> i64 {
    (task.jira_reported_ms - task.jira_link_baseline_ms).max(0)
}

/// Map issue key → linked task (prefer non-archived; name order from DB).
pub fn task_by_jira_key(tasks: &[Task]) -> HashMap<String, &Task> {
    let mut map = HashMap::new();
    // Pass 1: non-archived
    for task in tasks {
        if task.jira_issue_key.is_empty() || task.archived {
            continue;
        }
        map.entry(task.jira_issue_key.clone()).or_insert(task);
    }
    // Pass 2: archived only if key still free
    for task in tasks {
        if task.jira_issue_key.is_empty() || !task.archived {
            continue;
        }
        map.entry(task.jira_issue_key.clone()).or_insert(task);
    }
    map
}

/// Sum TimeTrove booked ms across all tasks currently linked to each issue key.
pub fn booked_ms_by_jira_key(tasks: &[Task]) -> HashMap<String, i64> {
    let mut map = HashMap::new();
    for task in tasks {
        if task.jira_issue_key.is_empty() {
            continue;
        }
        *map.entry(task.jira_issue_key.clone()).or_insert(0) += timetrove_booked_ms(task);
    }
    map
}

pub fn enrich_issues(
    issues: &[JiraIssueDetails],
    site_url: &str,
    linked_tasks: &[Task],
) -> Vec<JiraDashboardIssue> {
    let by_key = task_by_jira_key(linked_tasks);
    let booked_by_key = booked_ms_by_jira_key(linked_tasks);
    issues
        .iter()
        .map(|issue| {
            let browse = browse_url(site_url, &issue.issue_key).unwrap_or_default();
            let linked = by_key.get(&issue.issue_key).copied();
            JiraDashboardIssue {
                issue_key: issue.issue_key.clone(),
                summary: issue.summary.clone(),
                status: issue.status.clone(),
                assignee: issue.assignee.clone(),
                estimate_seconds: issue.estimate_seconds,
                total_spent_seconds: issue.total_spent_seconds,
                browse_url: browse,
                task_id: linked.map(|t| t.id.clone()),
                task_name: linked.map(|t| t.name.clone()),
                timetrove_booked_ms: booked_by_key.get(&issue.issue_key).copied(),
            }
        })
        .collect()
}

fn credentials_if_ready(db: &Db) -> Result<Option<JiraCredentials>, String> {
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
        proxy_url: settings.jira_proxy_url,
    }))
}

fn cache_is_fresh(fetched_at: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    match fetched_at {
        Some(at) => now - at < CACHE_TTL,
        None => false,
    }
}

/// List assigned open issues, using a 5-minute process-wide cache unless `force`.
pub fn list_assigned(db: &Db, force: bool) -> Result<JiraAssignedIssuesResult, String> {
    list_assigned_at(db, force, Utc::now())
}

pub fn list_assigned_at(
    db: &Db,
    force: bool,
    now: DateTime<Utc>,
) -> Result<JiraAssignedIssuesResult, String> {
    let settings = db.get_settings()?;
    if !settings.jira_enabled {
        return Ok(JiraAssignedIssuesResult {
            issues: vec![],
            fetched_at: None,
            truncated: false,
            from_cache: false,
            error: Some("Enable Jira sync in Settings".into()),
        });
    }

    let Some(creds) = credentials_if_ready(db)? else {
        return Ok(JiraAssignedIssuesResult {
            issues: vec![],
            fetched_at: None,
            truncated: false,
            from_cache: false,
            error: Some("Configure Jira site URL, email, and API token in Settings".into()),
        });
    };

    let account_id = normalize_account_id(&creds.email);
    let linked = db.list_tasks_with_jira_key()?;

    if crate::demo::enabled() {
        return Ok(JiraAssignedIssuesResult {
            issues: enrich_issues(&crate::demo::jira_fixture(), &creds.site_url, &linked),
            fetched_at: Some(now),
            truncated: false,
            from_cache: false,
            error: None,
        });
    }

    {
        let cache = CACHE.lock().map_err(|e| e.to_string())?;
        if !force
            && cache_is_fresh(cache.fetched_at, now)
            && cache_matches_identity(&cache, &creds.site_url, &account_id)
        {
            return Ok(JiraAssignedIssuesResult {
                issues: enrich_issues(&cache.issues, &cache.site_url, &linked),
                fetched_at: cache.fetched_at,
                truncated: cache.truncated,
                from_cache: true,
                error: None,
            });
        }
    }

    match search_my_open_issues(&creds, ASSIGNED_ISSUES_MAX) {
        Ok((issues, truncated)) => {
            let site = creds.site_url.clone();
            {
                let mut cache = CACHE.lock().map_err(|e| e.to_string())?;
                cache.fetched_at = Some(now);
                cache.issues = issues.clone();
                cache.truncated = truncated;
                cache.site_url = site.clone();
                cache.account_id = account_id;
            }
            let _ = db.set_jira_last_error(None);
            Ok(JiraAssignedIssuesResult {
                issues: enrich_issues(&issues, &site, &linked),
                fetched_at: Some(now),
                truncated,
                from_cache: false,
                error: None,
            })
        }
        Err(e) => {
            let _ = db.set_jira_last_error(Some(&e));
            // Serve stale cache on error only when site + account match.
            let cache = CACHE.lock().map_err(|err| err.to_string())?;
            if !cache.issues.is_empty()
                && cache_matches_identity(&cache, &creds.site_url, &account_id)
            {
                return Ok(JiraAssignedIssuesResult {
                    issues: enrich_issues(&cache.issues, &cache.site_url, &linked),
                    fetched_at: cache.fetched_at,
                    truncated: cache.truncated,
                    from_cache: true,
                    error: Some(e),
                });
            }
            Ok(JiraAssignedIssuesResult {
                issues: vec![],
                fetched_at: None,
                truncated: false,
                from_cache: false,
                error: Some(e),
            })
        }
    }
}

/// Clear the assigned-issues cache (tests).
#[cfg(test)]
pub fn clear_cache_for_tests() {
    if let Ok(mut cache) = CACHE.lock() {
        *cache = CacheInner::default();
    }
}

/// Seed the assigned-issues cache (tests).
#[cfg(test)]
fn seed_cache_for_tests(
    site_url: &str,
    email: &str,
    issues: Vec<JiraIssueDetails>,
    fetched_at: DateTime<Utc>,
) {
    if let Ok(mut cache) = CACHE.lock() {
        cache.fetched_at = Some(fetched_at);
        cache.issues = issues;
        cache.truncated = false;
        cache.site_url = site_url.to_string();
        cache.account_id = normalize_account_id(email);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::models::{CreateTaskInput, JiraIssueDetails, UpdateTaskInput};
    use crate::test_support::test_db;
    use chrono::TimeZone;
    use std::sync::{Mutex, MutexGuard};

    /// Serialize tests that mutate the process-wide CACHE (Rust runs tests in parallel).
    static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn lock_cache_tests() -> MutexGuard<'static, ()> {
        CACHE_TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn link_task(db: &Db, name: &str, key: &str) -> Task {
        let t = db
            .create_task(CreateTaskInput {
                name: name.into(),
                color: None,
                description: None,
                notes: None,
                jira_issue_key: None,
            })
            .unwrap();
        db.update_task(UpdateTaskInput {
            id: t.id.clone(),
            name: None,
            color: None,
            description: None,
            notes: None,
            archived: None,
            starred: None,
            jira_issue_key: Some(key.into()),
            jira_issue_details: Some(JiraIssueDetails {
                issue_key: key.into(),
                summary: format!("{name} summary"),
                status: "In Progress".into(),
                assignee: "Ada".into(),
                estimate_seconds: Some(7200),
                total_spent_seconds: Some(0),
            }),
        })
        .unwrap()
    }

    #[test]
    fn booked_ms_is_reported_minus_baseline() {
        let (_dir, db) = test_db();
        let t = link_task(&db, "Booked", "PROJ-1");
        let mut task = db.get_task(&t.id).unwrap().unwrap();
        task.jira_reported_ms = task.jira_link_baseline_ms + 90_000;
        assert_eq!(timetrove_booked_ms(&task), 90_000);
        task.jira_reported_ms = task.jira_link_baseline_ms - 10;
        assert_eq!(timetrove_booked_ms(&task), 0);
    }

    #[test]
    fn enrich_attaches_linked_task() {
        let (_dir, db) = test_db();
        let t = link_task(&db, "Widget", "WID-1");
        let linked = db.list_tasks_with_jira_key().unwrap();
        let issues = vec![JiraIssueDetails {
            issue_key: "WID-1".into(),
            summary: "Build widget".into(),
            status: "To Do".into(),
            assignee: "Ada".into(),
            estimate_seconds: Some(3600),
            total_spent_seconds: Some(0),
        }];
        let enriched = enrich_issues(&issues, "https://example.atlassian.net", &linked);
        assert_eq!(enriched.len(), 1);
        assert_eq!(enriched[0].task_name.as_deref(), Some("Widget"));
        assert_eq!(enriched[0].task_id.as_deref(), Some(t.id.as_str()));
        assert_eq!(
            enriched[0].browse_url,
            "https://example.atlassian.net/browse/WID-1"
        );
        assert!(enriched[0].timetrove_booked_ms.is_some());
    }

    #[test]
    fn enrich_sums_booked_ms_across_tasks_sharing_key() {
        let (_dir, db) = test_db();
        let a = link_task(&db, "Alpha", "PROJ-1");
        let b = link_task(&db, "Beta", "PROJ-1");
        db.commit_jira_reported_ms(&a.id, 3_600_000).unwrap();
        db.commit_jira_reported_ms(&b.id, 3_600_000).unwrap();
        let linked = db.list_tasks_with_jira_key().unwrap();
        let issues = vec![JiraIssueDetails {
            issue_key: "PROJ-1".into(),
            summary: "Shared".into(),
            status: "In Progress".into(),
            assignee: "Ada".into(),
            estimate_seconds: Some(14_400),
            total_spent_seconds: Some(7_200),
        }];
        let enriched = enrich_issues(&issues, "https://example.atlassian.net", &linked);
        assert_eq!(enriched[0].timetrove_booked_ms, Some(7_200_000));
        // Representative task is first non-archived by name order (Alpha before Beta).
        assert_eq!(enriched[0].task_name.as_deref(), Some("Alpha"));
    }

    #[test]
    fn cache_freshness() {
        let t0 = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        assert!(!cache_is_fresh(None, t0));
        assert!(cache_is_fresh(Some(t0), t0 + Duration::minutes(4)));
        assert!(!cache_is_fresh(Some(t0), t0 + Duration::minutes(5)));
    }

    #[test]
    fn cache_identity_requires_site_and_account() {
        let cache = CacheInner {
            fetched_at: Some(Utc::now()),
            issues: vec![],
            truncated: false,
            site_url: "https://a.atlassian.net".into(),
            account_id: "ada@example.com".into(),
        };
        assert!(cache_matches_identity(
            &cache,
            "https://a.atlassian.net",
            "ada@example.com"
        ));
        assert!(!cache_matches_identity(
            &cache,
            "https://a.atlassian.net",
            "bob@example.com"
        ));
        assert!(!cache_matches_identity(
            &cache,
            "https://b.atlassian.net",
            "ada@example.com"
        ));
        assert_eq!(normalize_account_id("  Ada@Example.com "), "ada@example.com");
    }

    #[test]
    fn list_assigned_uses_fresh_cache_for_matching_identity() {
        let _guard = lock_cache_tests();
        clear_cache_for_tests();
        let (_dir, db) = test_db();
        let mut settings = db.get_settings().unwrap();
        settings.jira_enabled = true;
        settings.jira_site_url = "https://example.atlassian.net".into();
        settings.jira_email = "Ada@Example.com".into();
        settings.jira_api_token = "token".into();
        db.update_settings(&settings).unwrap();

        let t0 = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        seed_cache_for_tests(
            "https://example.atlassian.net",
            "ada@example.com",
            vec![JiraIssueDetails {
                issue_key: "CACHED-1".into(),
                summary: "From cache".into(),
                status: "To Do".into(),
                assignee: "Ada".into(),
                estimate_seconds: None,
                total_spent_seconds: None,
            }],
            t0,
        );

        let result = list_assigned_at(&db, false, t0 + Duration::minutes(1)).unwrap();
        assert!(result.from_cache);
        assert!(result.error.is_none());
        assert_eq!(result.issues.len(), 1);
        assert_eq!(result.issues[0].issue_key, "CACHED-1");
        clear_cache_for_tests();
    }

    #[test]
    fn list_assigned_refuses_stale_cache_for_other_account() {
        let _guard = lock_cache_tests();
        clear_cache_for_tests();
        let (_dir, db) = test_db();
        let mut settings = db.get_settings().unwrap();
        settings.jira_enabled = true;
        settings.jira_site_url = "https://example.atlassian.net".into();
        settings.jira_email = "bob@example.com".into();
        settings.jira_api_token = "token".into();
        db.update_settings(&settings).unwrap();

        let t0 = Utc.with_ymd_and_hms(2026, 4, 1, 12, 0, 0).unwrap();
        seed_cache_for_tests(
            "https://example.atlassian.net",
            "ada@example.com",
            vec![JiraIssueDetails {
                issue_key: "ADA-1".into(),
                summary: "Ada only".into(),
                status: "To Do".into(),
                assignee: "Ada".into(),
                estimate_seconds: None,
                total_spent_seconds: None,
            }],
            t0,
        );

        // force=true skips fresh hit; HTTP fails; stale must not leak Ada's issues to Bob.
        let result = list_assigned_at(&db, true, t0 + Duration::minutes(1)).unwrap();
        assert!(!result.from_cache);
        assert!(result.issues.is_empty());
        assert!(result.error.is_some());
        clear_cache_for_tests();
    }

    #[test]
    fn list_assigned_disabled_nudge() {
        let _guard = lock_cache_tests();
        clear_cache_for_tests();
        let (_dir, db) = test_db();
        let result = list_assigned(&db, true).unwrap();
        assert!(result.issues.is_empty());
        assert!(result
            .error
            .as_deref()
            .unwrap_or("")
            .contains("Enable Jira"));
    }
}
