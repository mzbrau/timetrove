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
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
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
}
