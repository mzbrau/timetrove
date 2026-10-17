use crate::domain::{CopilotQuotaPool, CopilotUsageSummary};
use crate::github::{self, GithubHttpConfig};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct CopilotCredentials {
    pub github_host: String,
    pub api_token: String,
    pub username: String,
    /// Optional HTTP CONNECT proxy (`http://host:port` or with user:pass). Empty = none.
    pub proxy_url: String,
    pub monthly_limit_override: Option<f64>,
}

impl CopilotCredentials {
    /// Host + token + proxy + override identity used to reject stale poll results.
    /// Username is excluded so discovery fill does not invalidate an in-flight poll.
    pub fn config_matches(&self, other: &Self) -> bool {
        self.github_host.trim().eq_ignore_ascii_case(other.github_host.trim())
            && self.api_token.trim() == other.api_token.trim()
            && self.proxy_url.trim() == other.proxy_url.trim()
            && override_eq(self.monthly_limit_override, other.monthly_limit_override)
    }

    fn http_config(&self) -> GithubHttpConfig {
        GithubHttpConfig {
            github_host: self.github_host.clone(),
            api_token: self.api_token.clone(),
            proxy_url: self.proxy_url.clone(),
        }
    }
}

fn override_eq(a: Option<f64>, b: Option<f64>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(x), Some(y)) => x.to_bits() == y.to_bits(),
        _ => false,
    }
}

pub use github::api_base_url;

pub fn fetch_authenticated_login(creds: &CopilotCredentials) -> Result<String, String> {
    github::fetch_authenticated_login(&creds.http_config())
}

fn pool_from_snapshot(id: &str, snap: &Value) -> Option<CopilotQuotaPool> {
    if !snap.is_object() {
        return None;
    }
    let entitlement = snap
        .get("entitlement")
        .and_then(|v| v.as_f64())
        .or_else(|| snap.get("entitlement").and_then(|v| v.as_i64()).map(|n| n as f64))
        .unwrap_or(0.0);
    let remaining = snap
        .get("remaining")
        .and_then(|v| v.as_f64())
        .or_else(|| snap.get("remaining").and_then(|v| v.as_i64()).map(|n| n as f64))
        .or_else(|| {
            snap.get("quota_remaining")
                .and_then(|v| v.as_f64())
        })
        .unwrap_or(0.0);
    let unlimited = snap
        .get("unlimited")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let percent_remaining = snap
        .get("percent_remaining")
        .and_then(|v| v.as_f64())
        .unwrap_or_else(|| {
            if unlimited || entitlement <= 0.0 {
                100.0
            } else {
                ((remaining / entitlement) * 100.0).clamp(0.0, 100.0)
            }
        });
    let used = if unlimited {
        0.0
    } else {
        (entitlement - remaining).max(0.0)
    };
    let percent_used = if unlimited {
        0.0
    } else if entitlement > 0.0 {
        ((used / entitlement) * 100.0).clamp(0.0, 100.0)
    } else {
        (100.0 - percent_remaining).clamp(0.0, 100.0)
    };
    Some(CopilotQuotaPool {
        id: id.to_string(),
        entitlement,
        remaining,
        used,
        percent_remaining,
        percent_used,
        unlimited,
        overage_count: snap
            .get("overage_count")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.0),
        overage_permitted: snap
            .get("overage_permitted")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
    })
}

fn unit_from_body(body: &Value) -> String {
    let sku = body
        .get("access_type_sku")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if sku.to_ascii_lowercase().contains("credit") {
        "credits".into()
    } else {
        "requests".into()
    }
}

fn reset_date_from_body(body: &Value) -> Option<String> {
    body.get("quota_reset_date_utc")
        .and_then(|v| v.as_str())
        .or_else(|| body.get("quota_reset_date").and_then(|v| v.as_str()))
        .map(|s| s.to_string())
}

/// Apply optional user override when API entitlement is unusable.
pub fn apply_limit_override(
    mut summary: CopilotUsageSummary,
    override_limit: Option<f64>,
) -> CopilotUsageSummary {
    let usable_api =
        !summary.unlimited && summary.entitlement.is_finite() && summary.entitlement > 0.0;
    if usable_api {
        summary.limit_source = "api".into();
        return summary;
    }
    if let Some(limit) = override_limit.filter(|n| n.is_finite() && *n > 0.0) {
        let used = if summary.used.is_finite() {
            summary.used.max(0.0)
        } else {
            0.0
        };
        summary.entitlement = limit;
        summary.remaining = (limit - used).max(0.0);
        summary.percent_used = ((used / limit) * 100.0).clamp(0.0, 100.0);
        summary.unlimited = false;
        summary.limit_source = "override".into();
    } else if summary.unlimited {
        summary.limit_source = "unlimited".into();
        summary.percent_used = 0.0;
    } else {
        summary.limit_source = "unknown".into();
    }
    summary
}

fn summary_from_user_body(
    body: &Value,
    username: &str,
) -> Result<CopilotUsageSummary, String> {
    let plan = body
        .get("copilot_plan")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let snapshots = body
        .get("quota_snapshots")
        .and_then(|v| v.as_object())
        .ok_or_else(|| "Copilot response missing quota_snapshots".to_string())?;

    let mut pools = Vec::new();
    for (id, snap) in snapshots {
        if let Some(pool) = pool_from_snapshot(id, snap) {
            pools.push(pool);
        }
    }
    if pools.is_empty() {
        return Err("Copilot response had no quota pools".into());
    }

    let premium = pools
        .iter()
        .find(|p| p.id == "premium_interactions")
        .cloned()
        .or_else(|| pools.first().cloned())
        .ok_or_else(|| "No premium quota pool".to_string())?;

    let limit_source = if premium.unlimited {
        "unlimited"
    } else if premium.entitlement.is_finite() && premium.entitlement > 0.0 {
        "api"
    } else {
        "unknown"
    };

    Ok(CopilotUsageSummary {
        plan,
        username: username.to_string(),
        unit: unit_from_body(body),
        entitlement: premium.entitlement,
        used: premium.used,
        remaining: premium.remaining,
        percent_used: premium.percent_used,
        unlimited: premium.unlimited,
        overage_count: premium.overage_count,
        overage_permitted: premium.overage_permitted,
        reset_date: reset_date_from_body(body),
        pools,
        limit_source: limit_source.into(),
        fetched_at: None,
        last_synced_at: None,
        last_error: None,
    })
}

/// Individual-plan fallback: `/copilot_internal/v2/token` limited_user_quotas.
fn summary_from_v2_token(
    body: &Value,
    username: &str,
) -> Result<CopilotUsageSummary, String> {
    let quotas = body
        .get("limited_user_quotas")
        .or_else(|| body.get("limited_user_quota"))
        .and_then(|v| v.as_object())
        .ok_or_else(|| "v2/token response missing limited_user_quotas".to_string())?;

    // Prefer premium interaction storage shape used by VS Code extensions.
    let premium = quotas
        .get("copilot_premium_interaction")
        .or_else(|| quotas.get("premium_interactions"))
        .cloned();

    let (entitlement, remaining, used) = if let Some(p) = premium {
        if let Some(storage) = p.get("storage") {
            let remaining = storage
                .get("remaining")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let entitlement = storage
                .get("entitlement")
                .or_else(|| storage.get("quota"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let used = (entitlement - remaining).max(0.0);
            (entitlement, remaining, used)
        } else {
            let remaining = p.get("remaining").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let entitlement = p
                .get("entitlement")
                .or_else(|| p.get("quota"))
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let used = (entitlement - remaining).max(0.0);
            (entitlement, remaining, used)
        }
    } else {
        return Err("v2/token response missing premium quota".into());
    };

    let percent_used = if entitlement > 0.0 {
        ((used / entitlement) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let pool = CopilotQuotaPool {
        id: "premium_interactions".into(),
        entitlement,
        remaining,
        used,
        percent_remaining: (100.0 - percent_used).clamp(0.0, 100.0),
        percent_used,
        unlimited: false,
        overage_count: 0.0,
        overage_permitted: false,
    };
    let limit_source = if entitlement.is_finite() && entitlement > 0.0 {
        "api"
    } else {
        "unknown"
    };
    Ok(CopilotUsageSummary {
        plan: body
            .get("copilot_plan")
            .and_then(|v| v.as_str())
            .unwrap_or("individual")
            .to_string(),
        username: username.to_string(),
        unit: "requests".into(),
        entitlement,
        used,
        remaining,
        percent_used,
        unlimited: false,
        overage_count: 0.0,
        overage_permitted: false,
        reset_date: reset_date_from_body(body),
        pools: vec![pool],
        limit_source: limit_source.into(),
        fetched_at: None,
        last_synced_at: None,
        last_error: None,
    })
}

pub fn fetch_usage(
    creds: &CopilotCredentials,
) -> Result<(CopilotUsageSummary, String), String> {
    if creds.api_token.trim().is_empty() {
        return Err("GitHub API token is required".into());
    }
    let base = api_base_url(&creds.github_host)?;
    let agent = github::build_agent(&creds.proxy_url)?;

    let username = if creds.username.trim().is_empty() {
        fetch_authenticated_login(creds)?
    } else {
        creds.username.trim().to_string()
    };

    let user_url = format!("{base}/copilot_internal/user");
    match github::get_json(&agent, &user_url, &creds.api_token) {
        Ok(body) => {
            let raw = body.to_string();
            // API-native summary only; override applied at read / poll return.
            let summary = summary_from_user_body(&body, &username)?;
            Ok((summary, raw))
        }
        Err(user_err) => {
            let token_url = format!("{base}/copilot_internal/v2/token");
            match github::get_json(&agent, &token_url, &creds.api_token) {
                Ok(body) => {
                    let raw = body.to_string();
                    let summary = summary_from_v2_token(&body, &username)?;
                    Ok((summary, raw))
                }
                Err(token_err) => Err(format!(
                    "Could not read Copilot quota. /copilot_internal/user: {user_err}. \
                     /copilot_internal/v2/token: {token_err}. \
                     Prefer a fine-grained PAT with Copilot Requests, or a GitHub OAuth/`gh` token."
                )),
            }
        }
    }
}

pub fn test_connection(creds: &CopilotCredentials) -> Result<String, String> {
    let login = fetch_authenticated_login(creds)?;
    let (summary, _) = fetch_usage(&CopilotCredentials {
        username: login.clone(),
        ..creds.clone()
    })?;
    let unit = &summary.unit;
    if summary.unlimited {
        Ok(format!(
            "Connected as {login} ({plan}) — unlimited {unit}",
            plan = if summary.plan.is_empty() {
                "copilot"
            } else {
                &summary.plan
            }
        ))
    } else {
        Ok(format!(
            "Connected as {login} — {:.0}/{:.0} {unit} used ({:.0}%)",
            summary.used, summary.entitlement, summary.percent_used
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn override_when_unlimited() {
        let summary = CopilotUsageSummary {
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
            pools: vec![],
            limit_source: "api".into(),
            fetched_at: None,
            last_synced_at: None,
            last_error: None,
        };
        let out = apply_limit_override(summary, Some(300.0));
        assert_eq!(out.limit_source, "override");
        assert_eq!(out.entitlement, 300.0);
        assert!((out.percent_used - 4.0).abs() < 0.01);
        assert!(!out.unlimited);
    }

    #[test]
    fn api_entitlement_wins_over_override() {
        let summary = CopilotUsageSummary {
            plan: "business".into(),
            username: "me".into(),
            unit: "requests".into(),
            entitlement: 300.0,
            used: 90.0,
            remaining: 210.0,
            percent_used: 30.0,
            unlimited: false,
            overage_count: 0.0,
            overage_permitted: true,
            reset_date: None,
            pools: vec![],
            limit_source: "api".into(),
            fetched_at: None,
            last_synced_at: None,
            last_error: None,
        };
        let out = apply_limit_override(summary, Some(100.0));
        assert_eq!(out.limit_source, "api");
        assert_eq!(out.entitlement, 300.0);
        assert!((out.percent_used - 30.0).abs() < 0.01);
    }

    #[test]
    fn parse_quota_snapshots() {
        let body = serde_json::json!({
            "copilot_plan": "business",
            "quota_reset_date_utc": "2026-11-01T00:00:00.000Z",
            "quota_snapshots": {
                "premium_interactions": {
                    "entitlement": 300,
                    "remaining": 93,
                    "percent_remaining": 31.0,
                    "unlimited": false,
                    "overage_count": 0,
                    "overage_permitted": true
                },
                "chat": {
                    "entitlement": 0,
                    "remaining": 0,
                    "percent_remaining": 100.0,
                    "unlimited": true,
                    "overage_count": 0,
                    "overage_permitted": false
                }
            }
        });
        let summary = summary_from_user_body(&body, "octocat").unwrap();
        assert_eq!(summary.plan, "business");
        assert_eq!(summary.entitlement, 300.0);
        assert_eq!(summary.remaining, 93.0);
        assert!((summary.used - 207.0).abs() < 0.01);
        assert_eq!(summary.pools.len(), 2);
        assert_eq!(summary.limit_source, "api");
    }
}
