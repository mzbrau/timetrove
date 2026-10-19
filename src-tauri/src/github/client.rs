use super::http::{self, GithubHttpConfig};
use crate::domain::{GithubActivityDigest, GithubCommitSummary, GithubPrSummary};
use serde_json::{json, Value};
use std::collections::HashMap;

const PR_SEARCH_QUERY: &str = r#"
query($q: String!) {
  search(query: $q, type: ISSUE, first: 50) {
    nodes {
      ... on PullRequest {
        number
        title
        url
        createdAt
        reviewDecision
        repository { nameWithOwner }
        commits(last: 1) {
          nodes {
            commit {
              statusCheckRollup { state }
            }
          }
        }
        reviewThreads(first: 100) {
          nodes { isResolved }
        }
        latestReviews(first: 100) {
          nodes {
            state
            author { login }
          }
        }
        reviewRequests(first: 30) {
          nodes {
            requestedReviewer {
              ... on User { login }
            }
          }
        }
      }
    }
  }
}
"#;

/// Fetch authored PRs, reviewer PRs, and recent commits for the authenticated user.
pub fn fetch_activity(
    config: &GithubHttpConfig,
    username_hint: &str,
) -> Result<GithubActivityDigest, String> {
    if config.api_token.trim().is_empty() {
        return Err("GitHub API token is required".into());
    }
    let base = http::api_base_url(&config.github_host)?;
    let agent = http::build_agent(&config.proxy_url)?;

    let login = if username_hint.trim().is_empty() {
        http::fetch_authenticated_login(config)?
    } else {
        username_hint.trim().to_string()
    };

    let authored = search_prs(&agent, &base, &config.api_token, &format!("is:open is:pr author:{login}"))?;
    let requested = search_prs(
        &agent,
        &base,
        &config.api_token,
        &format!("is:open is:pr review-requested:{login}"),
    )?;
    let reviewed = search_prs(
        &agent,
        &base,
        &config.api_token,
        &format!("is:open is:pr reviewed-by:{login}"),
    )?;

    let mut review_map: HashMap<String, GithubPrSummary> = HashMap::new();
    for mut pr in requested {
        pr.needs_my_review = true;
        review_map.insert(pr.url.clone(), pr);
    }
    for mut pr in reviewed {
        let needs = viewer_needs_review(&pr, &login);
        pr.needs_my_review = needs;
        review_map
            .entry(pr.url.clone())
            .and_modify(|existing| {
                // Prefer keeping needs_my_review if either source says so.
                if pr.needs_my_review {
                    existing.needs_my_review = true;
                }
            })
            .or_insert(pr);
    }
    let mut review_prs: Vec<GithubPrSummary> = review_map.into_values().collect();
    review_prs.sort_by(|a, b| {
        b.needs_my_review
            .cmp(&a.needs_my_review)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    let mut authored_prs = authored;
    for pr in &mut authored_prs {
        pr.needs_my_review = false;
    }
    authored_prs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let recent_commits = fetch_recent_commits(&agent, &base, &config.api_token, &login)?;

    Ok(GithubActivityDigest {
        authored_prs,
        review_prs,
        recent_commits,
        fetched_at: None,
        last_synced_at: None,
        last_error: None,
        username: login,
    })
}

fn search_prs(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    query: &str,
) -> Result<Vec<GithubPrSummary>, String> {
    let body = http::post_graphql(agent, api_base, token, PR_SEARCH_QUERY, json!({ "q": query }))?;
    let nodes = body
        .pointer("/data/search/nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "GitHub GraphQL search response missing nodes".to_string())?;
    Ok(nodes.iter().filter_map(parse_pr_node).collect())
}

fn parse_pr_node(node: &Value) -> Option<GithubPrSummary> {
    if node.get("number").is_none() {
        return None;
    }
    let number = node.get("number")?.as_i64()? as i32;
    let title = node.get("title")?.as_str()?.to_string();
    let url = node.get("url")?.as_str()?.to_string();
    let created_at = node
        .get("createdAt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let repo_full_name = node
        .pointer("/repository/nameWithOwner")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let review_decision = node
        .get("reviewDecision")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let check_status = node
        .pointer("/commits/nodes/0/commit/statusCheckRollup/state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let unresolved = node
        .pointer("/reviewThreads/nodes")
        .and_then(|v| v.as_array())
        .map(|threads| {
            threads
                .iter()
                .filter(|t| t.get("isResolved").and_then(|v| v.as_bool()) == Some(false))
                .count() as u32
        })
        .unwrap_or(0);

    let reviews_summary = summarize_reviews(node.pointer("/latestReviews/nodes"));

    let mut requested_logins = Vec::new();
    if let Some(reqs) = node.pointer("/reviewRequests/nodes").and_then(|v| v.as_array()) {
        for req in reqs {
            if let Some(login) = req
                .pointer("/requestedReviewer/login")
                .and_then(|v| v.as_str())
            {
                requested_logins.push(login.to_string());
            }
        }
    }

    Some(GithubPrSummary {
        number,
        title,
        url,
        repo_full_name,
        created_at,
        check_status,
        review_decision,
        unresolved_comment_count: unresolved,
        reviews_summary,
        needs_my_review: false,
        requested_reviewer_logins: requested_logins,
    })
}

fn summarize_reviews(nodes: Option<&Value>) -> String {
    let Some(arr) = nodes.and_then(|v| v.as_array()) else {
        return "No reviews".into();
    };
    if arr.is_empty() {
        return "No reviews".into();
    }
    // Latest review per author (API returns roughly chronological; take last per login).
    let mut latest: HashMap<String, String> = HashMap::new();
    for r in arr {
        let login = r
            .pointer("/author/login")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let state = r
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("COMMENTED")
            .to_string();
        latest.insert(login.to_ascii_lowercase(), state);
    }
    let mut approved = 0u32;
    let mut changes = 0u32;
    let mut commented = 0u32;
    let mut pending = 0u32;
    for state in latest.values() {
        match state.as_str() {
            "APPROVED" => approved += 1,
            "CHANGES_REQUESTED" => changes += 1,
            "PENDING" => pending += 1,
            _ => commented += 1,
        }
    }
    let mut parts = Vec::new();
    if approved > 0 {
        parts.push(format!("{approved} approved"));
    }
    if changes > 0 {
        parts.push(format!("{changes} changes requested"));
    }
    if commented > 0 {
        parts.push(format!("{commented} commented"));
    }
    if pending > 0 {
        parts.push(format!("{pending} pending"));
    }
    if parts.is_empty() {
        "No reviews".into()
    } else {
        parts.join(" · ")
    }
}

fn viewer_needs_review(pr: &GithubPrSummary, login: &str) -> bool {
    let login_l = login.to_ascii_lowercase();
    pr.requested_reviewer_logins
        .iter()
        .any(|l| l.eq_ignore_ascii_case(&login_l))
}

fn fetch_recent_commits(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    login: &str,
) -> Result<Vec<GithubCommitSummary>, String> {
    let q = urlencoding_simple(&format!("author:{login}"));
    let url = format!(
        "{api_base}/search/commits?q={q}&sort=author-date&order=desc&per_page=20"
    );
    // Commits search historically required the cloak media type; keep it alongside API version.
    let body = http::get_json_accept(
        agent,
        &url,
        token,
        "application/vnd.github.cloak-preview+json",
    )
    .or_else(|_| http::get_json(agent, &url, token))?;

    let items = body
        .get("items")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "GitHub commit search response missing items".to_string())?;

    Ok(items.iter().filter_map(parse_commit_item).collect())
}

fn parse_commit_item(item: &Value) -> Option<GithubCommitSummary> {
    let sha = item.get("sha")?.as_str()?.to_string();
    let message = item
        .pointer("/commit/message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let committed_at = item
        .pointer("/commit/author/date")
        .and_then(|v| v.as_str())
        .or_else(|| item.pointer("/commit/committer/date").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let repo_full_name = item
        .pointer("/repository/full_name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let url = item
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(GithubCommitSummary {
        sha,
        message,
        repo_full_name,
        committed_at,
        url,
    })
}

fn urlencoding_simple(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(b as char);
            }
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_from_fixture() {
        let node = serde_json::json!({
            "number": 42,
            "title": "Fix idle gap",
            "url": "https://github.com/acme/app/pull/42",
            "createdAt": "2026-10-01T12:00:00Z",
            "reviewDecision": "REVIEW_REQUIRED",
            "repository": { "nameWithOwner": "acme/app" },
            "commits": {
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": { "state": "SUCCESS" }
                    }
                }]
            },
            "reviewThreads": {
                "nodes": [
                    { "isResolved": false },
                    { "isResolved": true },
                    { "isResolved": false }
                ]
            },
            "latestReviews": {
                "nodes": [
                    { "state": "COMMENTED", "author": { "login": "alice" } },
                    { "state": "APPROVED", "author": { "login": "bob" } }
                ]
            },
            "reviewRequests": {
                "nodes": [
                    { "requestedReviewer": { "login": "octocat" } }
                ]
            }
        });
        let pr = parse_pr_node(&node).unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.repo_full_name, "acme/app");
        assert_eq!(pr.check_status, "SUCCESS");
        assert_eq!(pr.unresolved_comment_count, 2);
        assert!(pr.reviews_summary.contains("approved"));
        assert_eq!(pr.requested_reviewer_logins, vec!["octocat".to_string()]);
        assert!(viewer_needs_review(&pr, "Octocat"));
        assert!(!viewer_needs_review(&pr, "alice"));
    }

    #[test]
    fn parse_commit_from_fixture() {
        let item = serde_json::json!({
            "sha": "abcdef0123456789",
            "html_url": "https://github.com/acme/app/commit/abcdef0123456789",
            "commit": {
                "message": "Add tracker bubble\n\nDetails here",
                "author": { "date": "2026-10-15T09:30:00Z" }
            },
            "repository": { "full_name": "acme/app" }
        });
        let c = parse_commit_item(&item).unwrap();
        assert_eq!(c.sha, "abcdef0123456789");
        assert_eq!(c.message, "Add tracker bubble");
        assert_eq!(c.repo_full_name, "acme/app");
        assert_eq!(c.committed_at, "2026-10-15T09:30:00Z");
    }

    #[test]
    fn summarize_empty_reviews() {
        assert_eq!(summarize_reviews(None), "No reviews");
        assert_eq!(summarize_reviews(Some(&json!([]))), "No reviews");
    }

    #[test]
    fn parse_pr_missing_checks_is_empty_status() {
        let node = serde_json::json!({
            "number": 7,
            "title": "Docs",
            "url": "https://github.com/acme/app/pull/7",
            "createdAt": "2026-10-01T12:00:00Z",
            "reviewDecision": "",
            "repository": { "nameWithOwner": "acme/app" },
            "commits": {
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": null
                    }
                }]
            },
            "reviewThreads": { "nodes": [] },
            "latestReviews": { "nodes": [] },
            "reviewRequests": { "nodes": [] }
        });
        let pr = parse_pr_node(&node).unwrap();
        assert_eq!(pr.check_status, "");
        assert_eq!(pr.unresolved_comment_count, 0);
        assert_eq!(pr.reviews_summary, "No reviews");
    }
}
