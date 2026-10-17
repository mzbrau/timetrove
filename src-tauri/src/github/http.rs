use serde_json::{json, Value};
use std::time::Duration;

/// Minimal credentials for GitHub REST / GraphQL HTTP.
#[derive(Debug, Clone)]
pub struct GithubHttpConfig {
    pub github_host: String,
    pub api_token: String,
    pub proxy_url: String,
}

/// Resolve REST/GraphQL API base URL for github.com or Enterprise Cloud (`*.ghe.com`).
pub fn api_base_url(host: &str) -> Result<String, String> {
    let trimmed = host.trim().trim_end_matches('/');
    let host = trimmed
        .strip_prefix("https://")
        .or_else(|| trimmed.strip_prefix("http://"))
        .unwrap_or(trimmed)
        .trim_end_matches('/');
    let host = host.strip_prefix("api.").unwrap_or(host);
    if host.is_empty() {
        return Err("GitHub host is required".into());
    }
    if host.contains('/') || host.contains(' ') {
        return Err("GitHub host should be a hostname like github.com or company.ghe.com".into());
    }
    if host.eq_ignore_ascii_case("github.com") {
        Ok("https://api.github.com".into())
    } else {
        Ok(format!("https://api.{host}"))
    }
}

/// Build a ureq Agent with connect/overall timeouts and optional HTTP proxy.
pub fn build_agent(proxy_url: &str) -> Result<ureq::Agent, String> {
    let mut builder = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(15))
        .timeout(Duration::from_secs(45));
    let proxy = proxy_url.trim();
    if !proxy.is_empty() {
        let parsed =
            ureq::Proxy::new(proxy).map_err(|e| format!("Invalid GitHub proxy URL: {e}"))?;
        builder = builder.proxy(parsed);
    }
    Ok(builder.build())
}

pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token.trim())
}

pub fn map_ureq_error(err: ureq::Error) -> String {
    match err {
        ureq::Error::Status(code, resp) => {
            let body = resp.into_string().unwrap_or_default();
            let snippet: String = body.chars().take(200).collect();
            if snippet.is_empty() {
                format!("GitHub returned HTTP {code}")
            } else {
                format!("GitHub returned HTTP {code}: {snippet}")
            }
        }
        ureq::Error::Transport(t) => format!("Network error talking to GitHub: {t}"),
    }
}

pub fn get_json(agent: &ureq::Agent, url: &str, token: &str) -> Result<Value, String> {
    get_json_accept(agent, url, token, "application/vnd.github+json")
}

pub fn get_json_accept(
    agent: &ureq::Agent,
    url: &str,
    token: &str,
    accept: &str,
) -> Result<Value, String> {
    let resp = agent
        .get(url)
        .set("Authorization", &auth_header(token))
        .set("Accept", accept)
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "TimeTrove")
        .call()
        .map_err(map_ureq_error)?;
    if !(200..300).contains(&resp.status()) {
        return Err(format!("GitHub returned HTTP {}", resp.status()));
    }
    resp.into_json().map_err(|e| e.to_string())
}

/// POST to `{api_base}/graphql` with a query and optional variables object.
pub fn post_graphql(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    query: &str,
    variables: Value,
) -> Result<Value, String> {
    let url = format!("{}/graphql", api_base.trim_end_matches('/'));
    let body = json!({ "query": query, "variables": variables });
    let resp = agent
        .post(&url)
        .set("Authorization", &auth_header(token))
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "TimeTrove")
        .set("Content-Type", "application/json")
        .send_json(body)
        .map_err(map_ureq_error)?;
    if !(200..300).contains(&resp.status()) {
        return Err(format!("GitHub GraphQL returned HTTP {}", resp.status()));
    }
    let value: Value = resp.into_json().map_err(|e| e.to_string())?;
    if let Some(errors) = value.get("errors").and_then(|e| e.as_array()) {
        if !errors.is_empty() {
            let msg = errors
                .iter()
                .filter_map(|e| e.get("message").and_then(|m| m.as_str()))
                .collect::<Vec<_>>()
                .join("; ");
            if value.get("data").map(|d| d.is_null()).unwrap_or(true) {
                return Err(if msg.is_empty() {
                    "GitHub GraphQL returned errors".into()
                } else {
                    format!("GitHub GraphQL: {msg}")
                });
            }
        }
    }
    Ok(value)
}

pub fn fetch_authenticated_login(config: &GithubHttpConfig) -> Result<String, String> {
    if config.api_token.trim().is_empty() {
        return Err("GitHub API token is required".into());
    }
    let base = api_base_url(&config.github_host)?;
    let agent = build_agent(&config.proxy_url)?;
    let body = get_json(&agent, &format!("{base}/user"), &config.api_token)?;
    body.get("login")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| "GitHub /user response missing login".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_base_github_com() {
        assert_eq!(api_base_url("github.com").unwrap(), "https://api.github.com");
        assert_eq!(
            api_base_url("https://github.com/").unwrap(),
            "https://api.github.com"
        );
        assert_eq!(
            api_base_url("api.github.com").unwrap(),
            "https://api.github.com"
        );
    }

    #[test]
    fn api_base_ghe() {
        assert_eq!(
            api_base_url("octocorp.ghe.com").unwrap(),
            "https://api.octocorp.ghe.com"
        );
        assert_eq!(
            api_base_url("https://octocorp.ghe.com").unwrap(),
            "https://api.octocorp.ghe.com"
        );
    }

    #[test]
    fn api_base_rejects_path() {
        assert!(api_base_url("github.com/foo").is_err());
        assert!(api_base_url("").is_err());
    }

    #[test]
    fn build_agent_ok_without_proxy() {
        assert!(build_agent("").is_ok());
        assert!(build_agent("   ").is_ok());
    }

    #[test]
    fn build_agent_ok_with_http_proxy() {
        assert!(build_agent("http://127.0.0.1:8080").is_ok());
        assert!(build_agent("http://user:pass@proxy.example:3128").is_ok());
    }

    #[test]
    fn build_agent_rejects_invalid_proxy() {
        let err = build_agent("ftp://proxy.example:8080").unwrap_err();
        assert!(err.contains("Invalid GitHub proxy URL"));
    }
}
