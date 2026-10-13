mod client;
mod sync;

pub use client::{fetch_issue, test_connection, JiraCredentials};
pub use sync::{flush_before_relink, spawn_flush_all, spawn_flush_task};

pub const MIN_WORKLOG_MS: i64 = 60_000;
pub const SYNC_INTERVAL_SECS: u64 = 3600;

#[cfg(test)]
mod tests {
    use super::client::normalize_site_url;
    use super::sync::{flush_before_relink, unreported_ms};
    use crate::db::Db;
    use crate::domain::models::{CreateTaskInput, UpdateTaskInput};
    use crate::test_support::test_db;
    use chrono::{Duration, Utc};

    #[test]
    fn unreported_delta_and_floor() {
        assert_eq!(unreported_ms(120_000, 0), 120_000);
        assert_eq!(unreported_ms(120_000, 50_000), 70_000);
        assert_eq!(unreported_ms(50_000, 0), 0); // under 60s
        assert_eq!(unreported_ms(59_999, 0), 0);
        assert_eq!(unreported_ms(60_000, 0), 60_000);
        assert_eq!(unreported_ms(100_000, 150_000), 0); // over-reported
    }

    #[test]
    fn normalize_site_strips_slash() {
        assert_eq!(
            normalize_site_url("https://example.atlassian.net/").unwrap(),
            "https://example.atlassian.net"
        );
        assert_eq!(
            normalize_site_url("https://example.atlassian.net").unwrap(),
            "https://example.atlassian.net"
        );
        assert!(normalize_site_url("").is_err());
        assert!(normalize_site_url("example.atlassian.net").is_err());
        assert!(normalize_site_url("http://example.atlassian.net").is_err());
    }

    fn link_task_with_slice(db: &Db) -> String {
        let t = db
            .create_task(CreateTaskInput {
                name: "FlushRelink".into(),
                color: Some("#E4572E".into()),
                description: None,
                notes: None,
                jira_issue_key: None,
            })
            .unwrap();
        let t0 = Utc::now() - Duration::minutes(5);
        let t1 = Utc::now() - Duration::minutes(3);
        db.insert_closed_slice(&t.id, t0, t1, "").unwrap();
        let total = db.task_total_ms(&t.id, t1).unwrap();
        db.update_task(UpdateTaskInput {
            id: t.id.clone(),
            name: None,
            color: None,
            description: None,
            notes: None,
            archived: None,
            starred: None,
            jira_issue_key: Some("OLD-1".into()),
            jira_issue_details: None,
        })
        .unwrap();
        // Baseline absorbed total at link; add more time so there is a postable delta.
        let t2 = Utc::now() - Duration::minutes(1);
        let t3 = Utc::now();
        db.insert_closed_slice(&t.id, t2, t3, "").unwrap();
        let _ = total;
        t.id
    }

    #[test]
    fn flush_before_relink_ok_when_nothing_due() {
        let (_dir, db) = test_db();
        let t = db
            .create_task(CreateTaskInput {
                name: "NothingDue".into(),
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
            jira_issue_key: Some("OLD-1".into()),
            jira_issue_details: None,
        })
        .unwrap();
        // No slices after baseline → nothing to flush even with Jira disabled.
        assert!(!db.get_settings().unwrap().jira_enabled);
        flush_before_relink(&db, &t.id).unwrap();
    }

    #[test]
    fn flush_before_relink_rejects_when_jira_disabled_and_due() {
        let (_dir, db) = test_db();
        let id = link_task_with_slice(&db);
        assert!(!db.get_settings().unwrap().jira_enabled);
        let err = flush_before_relink(&db, &id).unwrap_err();
        assert!(err.contains("Enable and configure Jira"));
        assert!(err.contains("OLD-1"));
    }
}
