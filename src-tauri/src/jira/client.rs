use crate::domain::JiraIssueDetails;
use chrono::{DateTime, Utc};
use serde::Serialize;

#[derive(Debug, Clone)]
pub struct JiraCredentials {
    pub site_url: String,
    pub email: String,
    pub api_token: String,
}

pub fn normalize_site_url(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim().trim_end_matches('/');
    if trimmed.is_empty() {
        return Err("Jira site URL is required".into());
    }
    if !trimmed.starts_with("https://") {
        return Err("Jira site URL must start with https://".into());
    }
    Ok(trimmed.to_string())
}

fn basic_auth_header(email: &str, token: &str) -> String {
    use base64::Engine as _;
    let raw = format!("{email}:{token}");
    format!(
        "Basic {}",
        base64::engine::general_purpose::STANDARD.encode(raw.as_bytes())
    )
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WorklogBody {
    time_spent_seconds: i64,
    started: String,
}

/// Jira Cloud expects `yyyy-MM-dd'T'HH:mm:ss.SSSZ` with numeric offset like `+0000`.
pub fn format_jira_started(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%dT%H:%M:%S%.3f%z").to_string()
}

pub fn test_connection(creds: &JiraCredentials) -> Result<String, String> {
    let base = normalize_site_url(&creds.site_url)?;
    if creds.email.trim().is_empty() {
        return Err("Jira email is required".into());
    }
    if creds.api_token.trim().is_empty() {
        return Err("Jira API token is required".into());
    }
    let url = format!("{base}/rest/api/3/myself");
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let resp = ureq::get(&url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .call()
        .map_err(map_ureq_error)?;
    if !(200..300).contains(&resp.status()) {
        return Err(format!("Jira returned HTTP {}", resp.status()));
    }
    let body: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    let display = body
        .get("displayName")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("emailAddress").and_then(|v| v.as_str()))
        .unwrap_or("ok");
    Ok(display.to_string())
}

pub fn add_worklog(
    creds: &JiraCredentials,
    issue_key: &str,
    time_spent_seconds: i64,
    started: DateTime<Utc>,
) -> Result<(), String> {
    if time_spent_seconds <= 0 {
        return Ok(());
    }
    let base = normalize_site_url(&creds.site_url)?;
    let url = format!("{base}/rest/api/3/issue/{issue_key}/worklog");
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let body = WorklogBody {
        time_spent_seconds,
        started: format_jira_started(started),
    };
    let resp = ureq::post(&url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(&body)
        .map_err(map_ureq_error)?;
    if !(200..300).contains(&resp.status()) {
        let status = resp.status();
        let detail = resp.into_string().unwrap_or_default();
        let snippet: String = detail.chars().take(200).collect();
        return Err(format!("Jira worklog failed (HTTP {status}): {snippet}"));
    }
    Ok(())
}

/// Fetch issue summary, status, assignee, estimate, and aggregate time spent.
pub fn fetch_issue(creds: &JiraCredentials, issue_key: &str) -> Result<JiraIssueDetails, String> {
    let key = issue_key.trim().to_uppercase();
    if key.is_empty() {
        return Err("Jira issue key is required".into());
    }
    let base = normalize_site_url(&creds.site_url)?;
    if creds.email.trim().is_empty() {
        return Err("Jira email is required".into());
    }
    if creds.api_token.trim().is_empty() {
        return Err("Jira API token is required".into());
    }
    let url = format!(
        "{base}/rest/api/3/issue/{key}?fields=summary,status,assignee,timetracking,timeoriginalestimate,aggregatetimespent"
    );
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let resp = ureq::get(&url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .call()
        .map_err(map_ureq_error)?;
    if resp.status() == 404 {
        return Err(format!("Jira issue {key} was not found"));
    }
    if !(200..300).contains(&resp.status()) {
        let status = resp.status();
        let detail = resp.into_string().unwrap_or_default();
        let snippet: String = detail.chars().take(200).collect();
        return Err(format!("Jira issue fetch failed (HTTP {status}): {snippet}"));
    }
    let body: serde_json::Value = resp.into_json().map_err(|e| e.to_string())?;
    parse_issue_json(&key, &body)
}

/// Parse a Jira issue JSON body into details (shared with unit tests).
pub fn parse_issue_json(fallback_key: &str, body: &serde_json::Value) -> Result<JiraIssueDetails, String> {
    let key = body
        .get("key")
        .and_then(|v| v.as_str())
        .unwrap_or(fallback_key)
        .to_uppercase();
    let fields = body
        .get("fields")
        .ok_or_else(|| "Jira issue response missing fields".to_string())?;
    let summary = fields
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let status = fields
        .get("status")
        .and_then(|s| s.get("name"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let assignee = fields
        .get("assignee")
        .and_then(|a| {
            if a.is_null() {
                None
            } else {
                a.get("displayName").and_then(|v| v.as_str())
            }
        })
        .unwrap_or("Unassigned")
        .to_string();
    let estimate_seconds = fields
        .get("timeoriginalestimate")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            fields
                .get("timetracking")
                .and_then(|t| t.get("originalEstimateSeconds"))
                .and_then(|v| v.as_i64())
        });
    let total_spent_seconds = fields
        .get("aggregatetimespent")
        .and_then(|v| v.as_i64())
        .or_else(|| {
            fields
                .get("timetracking")
                .and_then(|t| t.get("timeSpentSeconds"))
                .and_then(|v| v.as_i64())
        });
    Ok(JiraIssueDetails {
        issue_key: key,
        summary,
        status,
        assignee,
        estimate_seconds,
        total_spent_seconds,
    })
}

fn map_ureq_error(err: ureq::Error) -> String {
    match err {
        ureq::Error::Status(code, resp) => {
            let detail = resp.into_string().unwrap_or_default();
            let snippet: String = detail.chars().take(200).collect();
            format!("Jira HTTP {code}: {snippet}")
        }
        ureq::Error::Transport(t) => format!("Jira network error: {t}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn jira_started_format() {
        let dt = Utc.with_ymd_and_hms(2026, 3, 15, 14, 30, 0).unwrap();
        let s = format_jira_started(dt);
        assert!(s.starts_with("2026-03-15T14:30:00.000"));
        assert!(s.ends_with("+0000") || s.ends_with("Z"));
    }

    #[test]
    fn worklog_body_serde() {
        let body = WorklogBody {
            time_spent_seconds: 3600,
            started: "2026-03-15T14:30:00.000+0000".into(),
        };
        let v = serde_json::to_value(&body).unwrap();
        assert_eq!(v["timeSpentSeconds"], 3600);
        assert_eq!(v["started"], "2026-03-15T14:30:00.000+0000");
    }

    #[test]
    fn parse_issue_json_maps_fields() {
        let body = serde_json::json!({
            "key": "THING-42",
            "fields": {
                "summary": "Fix the widget",
                "status": { "name": "In Progress" },
                "assignee": { "displayName": "Ada Lovelace" },
                "timeoriginalestimate": 7200,
                "aggregatetimespent": 3600
            }
        });
        let d = parse_issue_json("THING-42", &body).unwrap();
        assert_eq!(d.issue_key, "THING-42");
        assert_eq!(d.summary, "Fix the widget");
        assert_eq!(d.status, "In Progress");
        assert_eq!(d.assignee, "Ada Lovelace");
        assert_eq!(d.estimate_seconds, Some(7200));
        assert_eq!(d.total_spent_seconds, Some(3600));
    }

    #[test]
    fn parse_issue_json_unassigned_and_timetracking_fallback() {
        let body = serde_json::json!({
            "key": "ABC-1",
            "fields": {
                "summary": "Untitled",
                "status": { "name": "To Do" },
                "assignee": null,
                "timetracking": {
                    "originalEstimateSeconds": 1800,
                    "timeSpentSeconds": 900
                }
            }
        });
        let d = parse_issue_json("ABC-1", &body).unwrap();
        assert_eq!(d.assignee, "Unassigned");
        assert_eq!(d.estimate_seconds, Some(1800));
        assert_eq!(d.total_spent_seconds, Some(900));
    }
}
