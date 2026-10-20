use crate::domain::JiraIssueDetails;
use chrono::{DateTime, Utc};
use serde::Serialize;
use std::time::Duration;

#[derive(Debug, Clone)]
pub struct JiraCredentials {
    pub site_url: String,
    pub email: String,
    pub api_token: String,
    /// Optional HTTP CONNECT proxy (`http://host:port` or with user:pass). Empty = none.
    pub proxy_url: String,
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

/// Build a ureq Agent with connect/overall timeouts and optional HTTP proxy.
pub fn build_agent(creds: &JiraCredentials) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(30));
    let proxy = creds.proxy_url.trim();
    if !proxy.is_empty() {
        let parsed = ureq::Proxy::new(proxy)
            .map_err(|e| format!("Invalid Jira proxy URL: {e}"))?;
        builder = builder.proxy(parsed);
    }
    Ok(builder.build())
}

fn validate_creds(creds: &JiraCredentials) -> Result<String, String> {
    let base = normalize_site_url(&creds.site_url)?;
    if creds.email.trim().is_empty() {
        return Err("Jira email is required".into());
    }
    if creds.api_token.trim().is_empty() {
        return Err("Jira API token is required".into());
    }
    // Validate proxy early so callers get a clear error before network I/O.
    let _ = build_agent(creds)?;
    Ok(base)
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
    let base = validate_creds(creds)?;
    let agent = build_agent(creds)?;
    let url = format!("{base}/rest/api/3/myself");
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let resp = agent
        .get(&url)
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
    let agent = build_agent(creds)?;
    let url = format!("{base}/rest/api/3/issue/{issue_key}/worklog");
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let body = WorklogBody {
        time_spent_seconds,
        started: format_jira_started(started),
    };
    let resp = agent
        .post(&url)
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

pub const ASSIGNED_ISSUES_MAX: u32 = 50;

pub const ASSIGNED_ISSUES_JQL: &str =
    "assignee = currentUser() AND statusCategory != Done ORDER BY updated DESC";

/// Build a Jira Cloud browse URL for an issue key.
pub fn browse_url(site_url: &str, issue_key: &str) -> Result<String, String> {
    let base = normalize_site_url(site_url)?;
    let key = issue_key.trim().to_uppercase();
    if key.is_empty() {
        return Err("Jira issue key is required".into());
    }
    Ok(format!("{base}/browse/{key}"))
}

/// Search open issues assigned to the current user (capped).
pub fn search_my_open_issues(
    creds: &JiraCredentials,
    max_results: u32,
) -> Result<(Vec<JiraIssueDetails>, bool), String> {
    let base = validate_creds(creds)?;
    let agent = build_agent(creds)?;
    let max = max_results.max(1);
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let fields = [
        "summary",
        "status",
        "assignee",
        "timetracking",
        "timeoriginalestimate",
        "aggregatetimespent",
        "comment",
    ];
    // Prefer the current Cloud search/jql endpoint; fall back to classic /search.
    let body = match post_search_jql(&agent, &base, &auth, max, &fields) {
        Ok(body) => body,
        Err(e) => {
            // Classic GET still works on some tenants.
            match get_search_classic(&agent, &base, &auth, max, &fields) {
                Ok(body) => body,
                Err(e2) => {
                    return Err(format!("{e}; classic search also failed: {e2}"));
                }
            }
        }
    };
    parse_search_json(&body, max)
}

fn post_search_jql(
    agent: &ureq::Agent,
    base: &str,
    auth: &str,
    max: u32,
    fields: &[&str],
) -> Result<serde_json::Value, String> {
    let url = format!("{base}/rest/api/3/search/jql");
    let payload = serde_json::json!({
        "jql": ASSIGNED_ISSUES_JQL,
        "maxResults": max,
        "fields": fields,
    });
    let resp = agent
        .post(&url)
        .set("Authorization", auth)
        .set("Accept", "application/json")
        .set("Content-Type", "application/json")
        .send_json(&payload)
        .map_err(map_ureq_error)?;
    if !(200..300).contains(&resp.status()) {
        let status = resp.status();
        let detail = resp.into_string().unwrap_or_default();
        let snippet: String = detail.chars().take(200).collect();
        return Err(format!("Jira search/jql failed (HTTP {status}): {snippet}"));
    }
    resp.into_json().map_err(|e| e.to_string())
}

fn get_search_classic(
    agent: &ureq::Agent,
    base: &str,
    auth: &str,
    max: u32,
    fields: &[&str],
) -> Result<serde_json::Value, String> {
    let fields_csv = fields.join(",");
    let url = format!(
        "{base}/rest/api/3/search?jql={}&maxResults={max}&fields={fields_csv}",
        urlencoding_jira_jql(ASSIGNED_ISSUES_JQL)
    );
    let resp = agent
        .get(&url)
        .set("Authorization", auth)
        .set("Accept", "application/json")
        .call()
        .map_err(map_ureq_error)?;
    if !(200..300).contains(&resp.status()) {
        let status = resp.status();
        let detail = resp.into_string().unwrap_or_default();
        let snippet: String = detail.chars().take(200).collect();
        return Err(format!("Jira search failed (HTTP {status}): {snippet}"));
    }
    resp.into_json().map_err(|e| e.to_string())
}

/// Minimal query escaping for JQL in a URL (spaces + reserved).
fn urlencoding_jira_jql(jql: &str) -> String {
    let mut out = String::with_capacity(jql.len() * 2);
    for b in jql.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push('%');
                out.push(HEX[(b >> 4) as usize] as char);
                out.push(HEX[(b & 0xf) as usize] as char);
            }
        }
    }
    out
}

const HEX: &[u8; 16] = b"0123456789ABCDEF";

/// Parse a Jira search response into issue details + truncated flag.
pub fn parse_search_json(
    body: &serde_json::Value,
    max_results: u32,
) -> Result<(Vec<JiraIssueDetails>, bool), String> {
    let issues_val = body
        .get("issues")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "Jira search response missing issues".to_string())?;
    let mut issues = Vec::with_capacity(issues_val.len());
    for issue in issues_val {
        let key = issue
            .get("key")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_uppercase();
        if key.is_empty() {
            continue;
        }
        issues.push(parse_issue_json(&key, issue)?);
    }
    let truncated = match body.get("total").and_then(|v| v.as_u64()) {
        Some(total) => total > issues.len() as u64,
        None => issues.len() as u32 >= max_results,
    };
    Ok((issues, truncated))
}

/// Fetch issue summary, status, assignee, estimate, and aggregate time spent.
pub fn fetch_issue(creds: &JiraCredentials, issue_key: &str) -> Result<JiraIssueDetails, String> {
    let key = issue_key.trim().to_uppercase();
    if key.is_empty() {
        return Err("Jira issue key is required".into());
    }
    let base = validate_creds(creds)?;
    let agent = build_agent(creds)?;
    let url = format!(
        "{base}/rest/api/3/issue/{key}?fields=summary,status,assignee,timetracking,timeoriginalestimate,aggregatetimespent"
    );
    let auth = basic_auth_header(creds.email.trim(), creds.api_token.trim());
    let resp = match agent
        .get(&url)
        .set("Authorization", &auth)
        .set("Accept", "application/json")
        .call()
    {
        Ok(resp) => resp,
        Err(ureq::Error::Status(404, _)) => {
            return Err(format!("Jira issue {key} was not found"));
        }
        Err(e) => return Err(map_ureq_error(e)),
    };
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
    let comment = fields.get("comment");
    let comment_total = comment
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let latest_comment_id = comment
        .and_then(|c| c.get("comments"))
        .and_then(|c| c.as_array())
        .and_then(|arr| arr.last())
        .and_then(|c| c.get("id"))
        .and_then(|v| {
            v.as_str()
                .map(|s| s.to_string())
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
        .unwrap_or_default();
    Ok(JiraIssueDetails {
        issue_key: key,
        summary,
        status,
        assignee,
        estimate_seconds,
        total_spent_seconds,
        comment_total,
        latest_comment_id,
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

    fn sample_creds(proxy: &str) -> JiraCredentials {
        JiraCredentials {
            site_url: "https://example.atlassian.net".into(),
            email: "dev@example.com".into(),
            api_token: "token".into(),
            proxy_url: proxy.into(),
        }
    }

    #[test]
    fn build_agent_ok_without_proxy() {
        assert!(build_agent(&sample_creds("")).is_ok());
        assert!(build_agent(&sample_creds("   ")).is_ok());
    }

    #[test]
    fn build_agent_ok_with_http_proxy() {
        assert!(build_agent(&sample_creds("http://127.0.0.1:8080")).is_ok());
        assert!(build_agent(&sample_creds("http://user:pass@proxy.example:3128")).is_ok());
    }

    #[test]
    fn build_agent_rejects_invalid_proxy() {
        // ureq only accepts http / socks* schemes when a scheme is present.
        let err = build_agent(&sample_creds("ftp://proxy.example:8080")).unwrap_err();
        assert!(err.contains("Invalid Jira proxy URL"));
    }

    #[test]
    fn browse_url_normalizes_site_and_key() {
        assert_eq!(
            browse_url("https://example.atlassian.net/", "thing-9").unwrap(),
            "https://example.atlassian.net/browse/THING-9"
        );
        assert!(browse_url("http://example.atlassian.net", "A-1").is_err());
        assert!(browse_url("https://example.atlassian.net", "").is_err());
    }

    #[test]
    fn parse_search_json_maps_issues_and_truncated() {
        let body = serde_json::json!({
            "total": 3,
            "issues": [
                {
                    "key": "A-1",
                    "fields": {
                        "summary": "One",
                        "status": { "name": "To Do" },
                        "assignee": { "displayName": "Ada" },
                        "timeoriginalestimate": 3600,
                        "aggregatetimespent": 600
                    }
                },
                {
                    "key": "B-2",
                    "fields": {
                        "summary": "Two",
                        "status": { "name": "In Progress" },
                        "assignee": null
                    }
                }
            ]
        });
        let (issues, truncated) = parse_search_json(&body, 50).unwrap();
        assert_eq!(issues.len(), 2);
        assert!(truncated);
        assert_eq!(issues[0].issue_key, "A-1");
        assert_eq!(issues[1].assignee, "Unassigned");
    }

    #[test]
    fn parse_search_json_not_truncated_when_total_matches() {
        let body = serde_json::json!({
            "total": 1,
            "issues": [{
                "key": "A-1",
                "fields": {
                    "summary": "One",
                    "status": { "name": "To Do" },
                    "assignee": null
                }
            }]
        });
        let (issues, truncated) = parse_search_json(&body, 50).unwrap();
        assert_eq!(issues.len(), 1);
        assert!(!truncated);
    }
}
