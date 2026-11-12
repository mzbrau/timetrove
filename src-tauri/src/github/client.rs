use super::http::{self, GithubHttpConfig};
use crate::domain::{
    GithubActivityDigest, GithubCommitSummary, GithubPrCheck, GithubPrDetail, GithubPrReview,
    GithubPrSummary, GithubPrThread, GithubPrTimelineItem, GithubPrTimelineKind,
};
use chrono::Utc;
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};

/// Identity-only search — keep complexity low so GitHub does not null out nodes.
const PR_SEARCH_QUERY: &str = r#"
query($q: String!) {
  search(query: $q, type: ISSUE, first: 50) {
    issueCount
    nodes {
      ... on PullRequest {
        id
        number
        title
        url
        createdAt
        reviewDecision
        repository { nameWithOwner }
      }
    }
  }
}
"#;

const VIEWER_AUTHORED_PRS_QUERY: &str = r#"
query {
  viewer {
    login
    pullRequests(states: OPEN, first: 50, orderBy: { field: CREATED_AT, direction: DESC }) {
      nodes {
        id
        number
        title
        url
        createdAt
        reviewDecision
        repository { nameWithOwner }
      }
    }
  }
}
"#;

const PR_HYDRATE_QUERY: &str = r#"
query($ids: [ID!]!) {
  nodes(ids: $ids) {
    ... on PullRequest {
      id
      headRefOid
      commits(last: 1) {
        totalCount
        nodes {
          commit {
            oid
            statusCheckRollup { state }
          }
        }
      }
      reviewThreads(first: 100) {
        nodes { isResolved }
        pageInfo { hasNextPage endCursor }
      }
      latestReviews(first: 100) {
        nodes {
          id
          submittedAt
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
"#;

const REVIEW_THREADS_PAGE_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!, $after: String!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $after) {
        nodes { isResolved }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;

/// On-demand PR detail for the expandable card (checks, reviews, threads, timeline).
const PR_DETAIL_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      number
      title
      url
      createdAt
      author { login }
      commits(last: 1) {
        nodes {
          commit {
            statusCheckRollup {
              state
              contexts(first: 100) {
                nodes {
                  __typename
                  ... on CheckRun {
                    name
                    status
                    conclusion
                    detailsUrl
                  }
                  ... on StatusContext {
                    context
                    state
                    description
                    targetUrl
                  }
                }
              }
            }
          }
        }
      }
      latestReviews(first: 100) {
        nodes {
          state
          submittedAt
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
      reviewThreads(first: 100) {
        nodes {
          isResolved
          path
          line
          comments(first: 1) {
            nodes {
              body
              createdAt
              author { login }
            }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
      timelineItems(
        last: 40
        itemTypes: [
          PULL_REQUEST_COMMIT
          ISSUE_COMMENT
          PULL_REQUEST_REVIEW_COMMENT
          PULL_REQUEST_REVIEW
          CLOSED_EVENT
          REOPENED_EVENT
          MERGED_EVENT
        ]
      ) {
        nodes {
          __typename
          ... on PullRequestCommit {
            commit {
              oid
              messageHeadline
              committedDate
              url
            }
          }
          ... on IssueComment {
            author { login }
            body
            createdAt
            url
          }
          ... on PullRequestReviewComment {
            author { login }
            body
            createdAt
            url
          }
          ... on PullRequestReview {
            author { login }
            state
            submittedAt
            body
            url
          }
          ... on ClosedEvent {
            createdAt
            actor { login }
          }
          ... on ReopenedEvent {
            createdAt
            actor { login }
          }
          ... on MergedEvent {
            createdAt
            actor { login }
          }
        }
      }
    }
  }
}
"#;

const DETAIL_THREADS_PAGE_QUERY: &str = r#"
query($owner: String!, $name: String!, $number: Int!, $after: String!) {
  repository(owner: $owner, name: $name) {
    pullRequest(number: $number) {
      reviewThreads(first: 100, after: $after) {
        nodes {
          isResolved
          path
          line
          comments(first: 1) {
            nodes {
              body
              createdAt
              author { login }
            }
          }
        }
        pageInfo { hasNextPage endCursor }
      }
    }
  }
}
"#;

/// Cap follow-up thread pages so a pathological PR cannot loop forever (100 × 50 = 5000 threads).
const MAX_REVIEW_THREAD_PAGES: usize = 50;
const MAX_RECENT_COMMITS: usize = 20;
const BODY_PREVIEW_MAX: usize = 200;

/// Fetch authored PRs, reviewer PRs, and recent commits for the authenticated user.
pub fn fetch_activity(
    config: &GithubHttpConfig,
    _username_hint: &str,
) -> Result<GithubActivityDigest, String> {
    if config.api_token.trim().is_empty() {
        return Err("GitHub API token is required".into());
    }
    let base = http::api_base_url(&config.github_host)?;
    let agent = http::build_agent(&config.proxy_url)?;

    // Always bind to the token — Settings username is not used for queries.
    let login = http::fetch_authenticated_login(config)?;

    let mut authored_prs = fetch_viewer_authored_prs(&agent, &base, &config.api_token)?;
    for pr in &mut authored_prs {
        pr.needs_my_review = false;
    }
    authored_prs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let requested = search_prs(
        &agent,
        &base,
        &config.api_token,
        "is:open is:pr review-requested:@me",
    )?;
    let reviewed = search_prs(
        &agent,
        &base,
        &config.api_token,
        "is:open is:pr reviewed-by:@me",
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

fn fetch_viewer_authored_prs(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
) -> Result<Vec<GithubPrSummary>, String> {
    let body = http::post_graphql(agent, api_base, token, VIEWER_AUTHORED_PRS_QUERY, json!({}))?;
    let nodes = body
        .pointer("/data/viewer/pullRequests/nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "GitHub GraphQL viewer.pullRequests missing nodes".to_string())?;
    let identities = parse_pr_identity_nodes(nodes)?;
    hydrate_prs(agent, api_base, token, identities)
}

fn search_prs(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    query: &str,
) -> Result<Vec<GithubPrSummary>, String> {
    let body = http::post_graphql(agent, api_base, token, PR_SEARCH_QUERY, json!({ "q": query }))?;
    let issue_count = body
        .pointer("/data/search/issueCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let nodes = body
        .pointer("/data/search/nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "GitHub GraphQL search response missing nodes".to_string())?;
    let identities = search_identities_or_err(issue_count, nodes)?;
    hydrate_prs(agent, api_base, token, identities)
}

#[derive(Debug)]
struct PrIdentity {
    id: String,
    summary: GithubPrSummary,
}

fn parse_pr_identity_nodes(nodes: &[Value]) -> Result<Vec<PrIdentity>, String> {
    let mut out = Vec::new();
    for node in nodes {
        if node.is_null() {
            continue;
        }
        if let Some(id) = parse_pr_identity(node) {
            out.push(id);
        }
    }
    if out.is_empty() && nodes.iter().any(|n| !n.is_null()) {
        return Err("pull request nodes missing required identity fields".into());
    }
    Ok(out)
}

fn parse_pr_identity(node: &Value) -> Option<PrIdentity> {
    if node.get("number").is_none() {
        return None;
    }
    let id = node.get("id")?.as_str()?.to_string();
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

    Some(PrIdentity {
        id,
        summary: GithubPrSummary {
            number,
            title,
            url,
            repo_full_name,
            created_at,
            check_status: String::new(),
            review_decision,
            unresolved_comment_count: 0,
            reviews_summary: "No reviews".into(),
            needs_my_review: false,
            head_oid: String::new(),
            commit_count: 0,
            requested_reviewer_logins: Vec::new(),
            reviews_fingerprint: String::new(),
        },
    })
}

fn hydrate_prs(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    identities: Vec<PrIdentity>,
) -> Result<Vec<GithubPrSummary>, String> {
    if identities.is_empty() {
        return Ok(Vec::new());
    }

    let order: Vec<String> = identities.iter().map(|p| p.id.clone()).collect();
    let mut by_id: HashMap<String, GithubPrSummary> = identities
        .into_iter()
        .map(|p| (p.id, p.summary))
        .collect();

    // Batch in chunks of 20 to stay within GraphQL complexity limits.
    for chunk in order.chunks(20) {
        let body = http::post_graphql(
            agent,
            api_base,
            token,
            PR_HYDRATE_QUERY,
            json!({ "ids": chunk }),
        )?;
        let nodes = body
            .pointer("/data/nodes")
            .and_then(|v| v.as_array())
            .ok_or_else(|| "GitHub GraphQL hydrate response missing nodes".to_string())?;
        for node in nodes {
            if node.is_null() {
                continue;
            }
            let Some(id) = node.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(pr) = by_id.get_mut(id) else {
                continue;
            };
            apply_hydration(pr, node);
            let first_page = node
                .get("reviewThreads")
                .cloned()
                .unwrap_or_else(|| json!({ "nodes": [] }));
            let repo = pr.repo_full_name.clone();
            let number = pr.number;
            pr.unresolved_comment_count = count_unresolved_across_pages(&first_page, |after| {
                fetch_review_threads_page(agent, api_base, token, &repo, number, after)
            })?;
        }
    }

    Ok(order
        .into_iter()
        .filter_map(|id| by_id.remove(&id))
        .collect())
}

fn apply_hydration(pr: &mut GithubPrSummary, node: &Value) {
    pr.check_status = node
        .pointer("/commits/nodes/0/commit/statusCheckRollup/state")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    pr.head_oid = node
        .get("headRefOid")
        .and_then(|v| v.as_str())
        .or_else(|| {
            node.pointer("/commits/nodes/0/commit/oid")
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string();
    pr.commit_count = node
        .pointer("/commits/totalCount")
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let reviews = node.pointer("/latestReviews/nodes");
    pr.reviews_summary = summarize_reviews(reviews);
    pr.reviews_fingerprint = reviews_fingerprint(reviews);
    pr.requested_reviewer_logins.clear();
    if let Some(reqs) = node.pointer("/reviewRequests/nodes").and_then(|v| v.as_array()) {
        for req in reqs {
            if let Some(login) = req
                .pointer("/requestedReviewer/login")
                .and_then(|v| v.as_str())
            {
                pr.requested_reviewer_logins.push(login.to_string());
            }
        }
    }
    // First-page unresolved count; pagination may refine in hydrate_prs.
    pr.unresolved_comment_count = count_unresolved_in_thread_nodes(
        node.pointer("/reviewThreads/nodes")
            .and_then(|v| v.as_array()),
    );
}

/// Parse a fully-hydrated PR node (identity + detail fields) — used by tests.
#[cfg(test)]
fn parse_pr_node(node: &Value) -> Option<GithubPrSummary> {
    let mut identity = parse_pr_identity(node)?;
    apply_hydration(&mut identity.summary, node);
    Some(identity.summary)
}

fn count_unresolved_in_thread_nodes(nodes: Option<&Vec<Value>>) -> u32 {
    nodes
        .map(|threads| {
            threads
                .iter()
                .filter(|t| t.get("isResolved").and_then(|v| v.as_bool()) == Some(false))
                .count() as u32
        })
        .unwrap_or(0)
}

fn next_thread_cursor(page: &Value) -> Option<String> {
    let has_next = page
        .pointer("/pageInfo/hasNextPage")
        .and_then(|v| v.as_bool())
        == Some(true);
    if !has_next {
        return None;
    }
    page.pointer("/pageInfo/endCursor")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
}

/// Count unresolved review threads across the first connection page and any follow-up pages.
fn count_unresolved_across_pages<F>(first_page: &Value, mut fetch_next: F) -> Result<u32, String>
where
    F: FnMut(&str) -> Result<Value, String>,
{
    let mut total =
        count_unresolved_in_thread_nodes(first_page.get("nodes").and_then(|v| v.as_array()));
    let mut cursor = next_thread_cursor(first_page);
    let mut pages = 1usize;
    while let Some(after) = cursor {
        if pages >= MAX_REVIEW_THREAD_PAGES {
            break;
        }
        let page = fetch_next(&after)?;
        total += count_unresolved_in_thread_nodes(page.get("nodes").and_then(|v| v.as_array()));
        cursor = next_thread_cursor(&page);
        pages += 1;
    }
    Ok(total)
}

fn split_owner_repo(full: &str) -> Option<(&str, &str)> {
    let (owner, name) = full.split_once('/')?;
    if owner.is_empty() || name.is_empty() || name.contains('/') {
        None
    } else {
        Some((owner, name))
    }
}

fn fetch_review_threads_page(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    repo_full_name: &str,
    number: i32,
    after: &str,
) -> Result<Value, String> {
    let (owner, name) = split_owner_repo(repo_full_name).ok_or_else(|| {
        format!("GitHub PR repo name is not owner/name: {repo_full_name}")
    })?;
    let body = http::post_graphql(
        agent,
        api_base,
        token,
        REVIEW_THREADS_PAGE_QUERY,
        json!({
            "owner": owner,
            "name": name,
            "number": number,
            "after": after,
        }),
    )?;
    body.pointer("/data/repository/pullRequest/reviewThreads")
        .cloned()
        .ok_or_else(|| "GitHub GraphQL reviewThreads page missing connection".to_string())
}

/// Fetch on-demand PR detail for the expandable GitHub card.
pub fn fetch_pr_detail(
    config: &GithubHttpConfig,
    repo_full_name: &str,
    number: i32,
) -> Result<GithubPrDetail, String> {
    if config.api_token.trim().is_empty() {
        return Err("GitHub API token is required".into());
    }
    let (owner, name) = split_owner_repo(repo_full_name).ok_or_else(|| {
        format!("GitHub PR repo name is not owner/name: {repo_full_name}")
    })?;
    let base = http::api_base_url(&config.github_host)?;
    let agent = http::build_agent(&config.proxy_url)?;
    let body = http::post_graphql(
        &agent,
        &base,
        &config.api_token,
        PR_DETAIL_QUERY,
        json!({
            "owner": owner,
            "name": name,
            "number": number,
        }),
    )?;
    let pr_node = body
        .pointer("/data/repository/pullRequest")
        .ok_or_else(|| {
            if body.pointer("/data/repository").is_some() {
                format!("Pull request #{number} not found in {repo_full_name}")
            } else {
                format!("Repository not found: {repo_full_name}")
            }
        })?;
    if pr_node.is_null() {
        return Err(format!("Pull request #{number} not found in {repo_full_name}"));
    }
    let mut detail = parse_pr_detail(pr_node, repo_full_name)?;

    // Paginate unresolved threads if the first page is incomplete.
    let first_page = pr_node
        .get("reviewThreads")
        .cloned()
        .unwrap_or_else(|| json!({ "nodes": [] }));
    let mut threads = parse_open_threads(first_page.get("nodes").and_then(|v| v.as_array()));
    let mut cursor = next_thread_cursor(&first_page);
    let mut pages = 1usize;
    while let Some(after) = cursor {
        if pages >= MAX_REVIEW_THREAD_PAGES {
            break;
        }
        let page = fetch_detail_threads_page(
            &agent,
            &base,
            &config.api_token,
            repo_full_name,
            number,
            &after,
        )?;
        threads.extend(parse_open_threads(page.get("nodes").and_then(|v| v.as_array())));
        cursor = next_thread_cursor(&page);
        pages += 1;
    }
    detail.open_threads = threads;
    detail.fetched_at = Utc::now().to_rfc3339();
    Ok(detail)
}

fn fetch_detail_threads_page(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    repo_full_name: &str,
    number: i32,
    after: &str,
) -> Result<Value, String> {
    let (owner, name) = split_owner_repo(repo_full_name).ok_or_else(|| {
        format!("GitHub PR repo name is not owner/name: {repo_full_name}")
    })?;
    let body = http::post_graphql(
        agent,
        api_base,
        token,
        DETAIL_THREADS_PAGE_QUERY,
        json!({
            "owner": owner,
            "name": name,
            "number": number,
            "after": after,
        }),
    )?;
    body.pointer("/data/repository/pullRequest/reviewThreads")
        .cloned()
        .ok_or_else(|| "GitHub GraphQL detail reviewThreads page missing connection".to_string())
}

fn parse_pr_detail(node: &Value, repo_full_name: &str) -> Result<GithubPrDetail, String> {
    let number = node
        .get("number")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| "PR detail missing number".to_string())? as i32;
    let title = node
        .get("title")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let url = node
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let created_at = node
        .get("createdAt")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let author = node
        .pointer("/author/login")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let checks = parse_checks(
        node.pointer("/commits/nodes/0/commit/statusCheckRollup/contexts/nodes")
            .and_then(|v| v.as_array()),
    );
    let reviews = parse_detail_reviews(node.pointer("/latestReviews/nodes").and_then(|v| v.as_array()));
    let mut pending_reviewers = Vec::new();
    if let Some(reqs) = node.pointer("/reviewRequests/nodes").and_then(|v| v.as_array()) {
        for req in reqs {
            if let Some(login) = req
                .pointer("/requestedReviewer/login")
                .and_then(|v| v.as_str())
            {
                pending_reviewers.push(login.to_string());
            }
        }
    }
    let open_threads =
        parse_open_threads(node.pointer("/reviewThreads/nodes").and_then(|v| v.as_array()));
    let mut timeline = parse_timeline(node.pointer("/timelineItems/nodes").and_then(|v| v.as_array()));

    // Synthetic "Opened" event — GraphQL timeline has no dedicated open item type.
    if !created_at.is_empty() {
        timeline.push(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Opened,
            at: created_at,
            actor: author,
            summary: "Pull request opened".into(),
            url: url.clone(),
        });
    }
    // Newest first.
    timeline.sort_by(|a, b| b.at.cmp(&a.at));

    Ok(GithubPrDetail {
        number,
        repo_full_name: repo_full_name.to_string(),
        title,
        url,
        checks,
        reviews,
        pending_reviewers,
        open_threads,
        timeline,
        fetched_at: String::new(),
    })
}

fn parse_checks(nodes: Option<&Vec<Value>>) -> Vec<GithubPrCheck> {
    let Some(arr) = nodes else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|n| {
            let typename = n.get("__typename").and_then(|v| v.as_str()).unwrap_or("");
            match typename {
                "CheckRun" => {
                    let name = n.get("name").and_then(|v| v.as_str()).unwrap_or("Check").to_string();
                    let status = n.get("status").and_then(|v| v.as_str()).unwrap_or("");
                    let conclusion = n.get("conclusion").and_then(|v| v.as_str()).unwrap_or("");
                    let state = normalize_check_run_state(status, conclusion);
                    let url = n
                        .get("detailsUrl")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(GithubPrCheck {
                        name,
                        state,
                        description: conclusion.to_string(),
                        url,
                    })
                }
                "StatusContext" => {
                    let name = n
                        .get("context")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Status")
                        .to_string();
                    let state = n
                        .get("state")
                        .and_then(|v| v.as_str())
                        .unwrap_or("PENDING")
                        .to_string();
                    let description = n
                        .get("description")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let url = n
                        .get("targetUrl")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    Some(GithubPrCheck {
                        name,
                        state,
                        description,
                        url,
                    })
                }
                _ => None,
            }
        })
        .collect()
}

fn normalize_check_run_state(status: &str, conclusion: &str) -> String {
    let status_u = status.to_ascii_uppercase();
    if status_u != "COMPLETED" {
        return match status_u.as_str() {
            "QUEUED" | "IN_PROGRESS" | "WAITING" | "PENDING" | "REQUESTED" => {
                "PENDING".into()
            }
            _ => {
                if status_u.is_empty() {
                    "PENDING".into()
                } else {
                    status_u
                }
            }
        };
    }
    match conclusion.to_ascii_uppercase().as_str() {
        "SUCCESS" => "SUCCESS".into(),
        "FAILURE" | "TIMED_OUT" | "STARTUP_FAILURE" | "ACTION_REQUIRED" => "FAILURE".into(),
        "CANCELLED" | "SKIPPED" | "NEUTRAL" | "STALE" => {
            if conclusion.is_empty() {
                "NEUTRAL".into()
            } else {
                conclusion.to_ascii_uppercase()
            }
        }
        other if !other.is_empty() => other.to_string(),
        _ => "PENDING".into(),
    }
}

fn parse_detail_reviews(nodes: Option<&Vec<Value>>) -> Vec<GithubPrReview> {
    let Some(arr) = nodes else {
        return Vec::new();
    };
    // Latest review per author (API is roughly chronological; last wins).
    let mut latest: HashMap<String, GithubPrReview> = HashMap::new();
    for r in arr {
        let author = r
            .pointer("/author/login")
            .and_then(|v| v.as_str())
            .unwrap_or("?")
            .to_string();
        let state = r
            .get("state")
            .and_then(|v| v.as_str())
            .unwrap_or("COMMENTED")
            .to_string();
        let submitted_at = r
            .get("submittedAt")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let key = author.to_ascii_lowercase();
        latest.insert(
            key,
            GithubPrReview {
                author,
                state,
                submitted_at,
            },
        );
    }
    let mut out: Vec<_> = latest.into_values().collect();
    out.sort_by(|a, b| a.author.to_ascii_lowercase().cmp(&b.author.to_ascii_lowercase()));
    out
}

fn parse_open_threads(nodes: Option<&Vec<Value>>) -> Vec<GithubPrThread> {
    let Some(arr) = nodes else {
        return Vec::new();
    };
    arr.iter()
        .filter(|t| t.get("isResolved").and_then(|v| v.as_bool()) == Some(false))
        .map(|t| {
            let path = t.get("path").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let line = t.get("line").and_then(|v| v.as_i64()).map(|n| n as i32);
            let comment = t.pointer("/comments/nodes/0");
            let author = comment
                .and_then(|c| c.pointer("/author/login"))
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string();
            let body_raw = comment
                .and_then(|c| c.get("body"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let created_at = comment
                .and_then(|c| c.get("createdAt"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            GithubPrThread {
                path,
                line,
                author,
                body: truncate_body(body_raw),
                created_at,
            }
        })
        .collect()
}

fn parse_timeline(nodes: Option<&Vec<Value>>) -> Vec<GithubPrTimelineItem> {
    let Some(arr) = nodes else {
        return Vec::new();
    };
    arr.iter().filter_map(parse_timeline_node).collect()
}

fn parse_timeline_node(n: &Value) -> Option<GithubPrTimelineItem> {
    let typename = n.get("__typename").and_then(|v| v.as_str())?;
    match typename {
        "PullRequestCommit" => {
            let oid = n.pointer("/commit/oid").and_then(|v| v.as_str()).unwrap_or("");
            let short = if oid.len() >= 7 { &oid[..7] } else { oid };
            let message = n
                .pointer("/commit/messageHeadline")
                .and_then(|v| v.as_str())
                .unwrap_or("(no message)");
            let at = n
                .pointer("/commit/committedDate")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = n
                .pointer("/commit/url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(GithubPrTimelineItem {
                kind: GithubPrTimelineKind::Commit,
                at,
                actor: String::new(),
                summary: format!("{short} {message}"),
                url,
            })
        }
        "IssueComment" => Some(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Comment,
            at: n.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor: n
                .pointer("/author/login")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            summary: truncate_body(n.get("body").and_then(|v| v.as_str()).unwrap_or("")),
            url: n.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "PullRequestReviewComment" => Some(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::ReviewComment,
            at: n.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor: n
                .pointer("/author/login")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            summary: truncate_body(n.get("body").and_then(|v| v.as_str()).unwrap_or("")),
            url: n.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        }),
        "PullRequestReview" => {
            let state = n.get("state").and_then(|v| v.as_str()).unwrap_or("COMMENTED");
            let body = n.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let summary = if body.trim().is_empty() {
                format!("Review: {state}")
            } else {
                format!("Review: {state} — {}", truncate_body(body))
            };
            Some(GithubPrTimelineItem {
                kind: GithubPrTimelineKind::Review,
                at: n
                    .get("submittedAt")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string(),
                actor: n
                    .pointer("/author/login")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string(),
                summary,
                url: n.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
        }
        "ClosedEvent" => Some(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Closed,
            at: n.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor: n
                .pointer("/actor/login")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            summary: "Pull request closed".into(),
            url: String::new(),
        }),
        "ReopenedEvent" => Some(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Reopened,
            at: n.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor: n
                .pointer("/actor/login")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            summary: "Pull request reopened".into(),
            url: String::new(),
        }),
        "MergedEvent" => Some(GithubPrTimelineItem {
            kind: GithubPrTimelineKind::Merged,
            at: n.get("createdAt").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            actor: n
                .pointer("/actor/login")
                .and_then(|v| v.as_str())
                .unwrap_or("?")
                .to_string(),
            summary: "Pull request merged".into(),
            url: String::new(),
        }),
        _ => None,
    }
}

fn truncate_body(s: &str) -> String {
    let trimmed = s.trim();
    if trimmed.chars().count() <= BODY_PREVIEW_MAX {
        return trimmed.to_string();
    }
    let preview: String = trimmed.chars().take(BODY_PREVIEW_MAX).collect();
    format!("{preview}…")
}

/// Compact fingerprint of latest review nodes (IDs, falling back to submittedAt+login).
fn reviews_fingerprint(nodes: Option<&Value>) -> String {
    let Some(arr) = nodes.and_then(|v| v.as_array()) else {
        return String::new();
    };
    let mut parts: Vec<String> = arr
        .iter()
        .map(|r| {
            if let Some(id) = r.get("id").and_then(|v| v.as_str()) {
                return id.to_string();
            }
            let submitted = r
                .get("submittedAt")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let login = r
                .pointer("/author/login")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            format!("{submitted}:{login}")
        })
        .filter(|s| !s.is_empty())
        .collect();
    parts.sort_unstable();
    parts.join("|")
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
        "{api_base}/search/commits?q={q}&sort=author-date&order=desc&per_page={MAX_RECENT_COMMITS}"
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

    let from_search: Vec<GithubCommitSummary> =
        items.iter().filter_map(parse_commit_item).collect();
    if !from_search.is_empty() {
        return Ok(from_search);
    }

    // Search often returns empty for fine-grained PATs / SSO; fall back to public events.
    fetch_commits_from_events(agent, api_base, token, login)
}

fn fetch_commits_from_events(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    login: &str,
) -> Result<Vec<GithubCommitSummary>, String> {
    let url = format!("{api_base}/users/{login}/events?per_page=100");
    let body = http::get_json(agent, &url, token)?;
    let events = body
        .as_array()
        .ok_or_else(|| "GitHub user events response is not an array".to_string())?;
    // Events API no longer embeds commit summaries (Oct 2025); only head/before SHAs remain.
    let refs = push_head_refs_from_events(events);
    let mut out = Vec::with_capacity(refs.len());
    for r in refs {
        match fetch_repo_commit(agent, api_base, token, &r.repo_full_name, &r.sha) {
            Ok(Some(commit)) => out.push(commit),
            Ok(None) | Err(_) => {
                // Skip private/inaccessible repos; keep collecting from later events.
                continue;
            }
        }
        if out.len() >= MAX_RECENT_COMMITS {
            break;
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PushHeadRef {
    repo_full_name: String,
    sha: String,
}

/// Collect tip SHAs from modern PushEvent payloads (`payload.head`, not removed `payload.commits`).
fn push_head_refs_from_events(events: &[Value]) -> Vec<PushHeadRef> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for event in events {
        if event.get("type").and_then(|v| v.as_str()) != Some("PushEvent") {
            continue;
        }
        let Some(repo_full_name) = event.pointer("/repo/name").and_then(|v| v.as_str()) else {
            continue;
        };
        if repo_full_name.is_empty() || split_owner_repo(repo_full_name).is_none() {
            continue;
        }
        let Some(sha) = event
            .pointer("/payload/head")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        if !seen.insert(sha.to_string()) {
            continue;
        }
        out.push(PushHeadRef {
            repo_full_name: repo_full_name.to_string(),
            sha: sha.to_string(),
        });
        if out.len() >= MAX_RECENT_COMMITS {
            break;
        }
    }
    out
}

fn fetch_repo_commit(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    repo_full_name: &str,
    sha: &str,
) -> Result<Option<GithubCommitSummary>, String> {
    let (owner, name) = split_owner_repo(repo_full_name).ok_or_else(|| {
        format!("GitHub commit repo name is not owner/name: {repo_full_name}")
    })?;
    let url = format!("{api_base}/repos/{owner}/{name}/commits/{sha}");
    let body = http::get_json(agent, &url, token)?;
    Ok(parse_repo_commit(&body, repo_full_name))
}

/// Parse `GET /repos/{owner}/{repo}/commits/{ref}` — uses API `html_url` (host-correct for GHE).
fn parse_repo_commit(body: &Value, repo_full_name: &str) -> Option<GithubCommitSummary> {
    let sha = body.get("sha")?.as_str()?.to_string();
    let message = body
        .pointer("/commit/message")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .lines()
        .next()
        .unwrap_or("")
        .to_string();
    let committed_at = body
        .pointer("/commit/author/date")
        .and_then(|v| v.as_str())
        .or_else(|| body.pointer("/commit/committer/date").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    let url = body
        .get("html_url")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some(GithubCommitSummary {
        sha,
        message,
        repo_full_name: repo_full_name.to_string(),
        committed_at,
        url,
    })
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

/// Pure helper for tests: validate search issueCount vs parsed identities.
fn search_identities_or_err(issue_count: u64, nodes: &[Value]) -> Result<Vec<PrIdentity>, String> {
    let identities = match parse_pr_identity_nodes(nodes) {
        Ok(list) => list,
        Err(e) => {
            if issue_count > 0 {
                return Err(format!(
                    "GitHub search reported {issue_count} hit(s) but none could be parsed ({e}). \
                     Check token scopes/SSO, or try again."
                ));
            }
            return Err(e);
        }
    };
    if issue_count > 0 && identities.is_empty() {
        return Err(format!(
            "GitHub search reported {issue_count} hit(s) but returned no usable pull requests. \
             Check token scopes/SSO (Issues + Pull requests read), or authorize the token for org SSO."
        ));
    }
    Ok(identities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pr_from_fixture() {
        let node = serde_json::json!({
            "id": "PR_kwDO_test",
            "number": 42,
            "title": "Fix idle gap",
            "url": "https://github.com/acme/app/pull/42",
            "createdAt": "2026-10-01T12:00:00Z",
            "reviewDecision": "REVIEW_REQUIRED",
            "repository": { "nameWithOwner": "acme/app" },
            "headRefOid": "deadbeef01",
            "commits": {
                "totalCount": 4,
                "nodes": [{
                    "commit": {
                        "oid": "deadbeef01",
                        "statusCheckRollup": { "state": "SUCCESS" }
                    }
                }]
            },
            "reviewThreads": {
                "nodes": [
                    { "isResolved": false },
                    { "isResolved": true },
                    { "isResolved": false }
                ],
                "pageInfo": { "hasNextPage": false, "endCursor": null }
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
        assert_eq!(pr.head_oid, "deadbeef01");
        assert_eq!(pr.commit_count, 4);
        assert_eq!(pr.unresolved_comment_count, 2);
        assert!(pr.reviews_summary.contains("approved"));
        assert_eq!(pr.requested_reviewer_logins, vec!["octocat".to_string()]);
        assert!(viewer_needs_review(&pr, "Octocat"));
        assert!(!viewer_needs_review(&pr, "alice"));
    }

    #[test]
    fn parse_pr_identity_only() {
        let node = serde_json::json!({
            "id": "PR_kwDO_id",
            "number": 9,
            "title": "Slim",
            "url": "https://github.com/acme/app/pull/9",
            "createdAt": "2026-10-01T12:00:00Z",
            "reviewDecision": "REVIEW_REQUIRED",
            "repository": { "nameWithOwner": "acme/app" }
        });
        let id = parse_pr_identity(&node).unwrap();
        assert_eq!(id.id, "PR_kwDO_id");
        assert_eq!(id.summary.number, 9);
        assert_eq!(id.summary.check_status, "");
        assert_eq!(id.summary.reviews_summary, "No reviews");
    }

    #[test]
    fn issue_count_with_null_nodes_is_error() {
        let nodes = vec![Value::Null, Value::Null];
        let err = search_identities_or_err(3, &nodes).unwrap_err();
        assert!(err.contains("3 hit"));
        assert!(err.contains("no usable") || err.contains("none could be parsed"));
    }

    #[test]
    fn issue_count_zero_with_empty_nodes_ok() {
        let ids = search_identities_or_err(0, &[]).unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn counts_unresolved_thread_on_later_page() {
        // First page is full of resolved threads; the only open thread is on page 2.
        let page1_nodes: Vec<Value> = (0..100).map(|_| json!({ "isResolved": true })).collect();
        let page1 = json!({
            "nodes": page1_nodes,
            "pageInfo": { "hasNextPage": true, "endCursor": "cursor-page-1" }
        });
        let page2 = json!({
            "nodes": [
                { "isResolved": false },
                { "isResolved": true }
            ],
            "pageInfo": { "hasNextPage": false, "endCursor": "cursor-page-2" }
        });
        let mut remaining = vec![page2];
        let count = count_unresolved_across_pages(&page1, |after| {
            assert_eq!(after, "cursor-page-1");
            remaining
                .pop()
                .ok_or_else(|| "unexpected extra page fetch".into())
        })
        .unwrap();
        assert_eq!(count, 1);
        assert!(remaining.is_empty());
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
    fn push_head_refs_from_modern_events() {
        // Post–Oct 2025 Events API: no payload.commits; only head/before remain.
        let events = json!([
            {
                "type": "WatchEvent",
                "created_at": "2026-10-18T10:00:00Z",
                "repo": { "name": "acme/other" },
                "payload": {}
            },
            {
                "type": "PushEvent",
                "created_at": "2026-10-18T12:00:00Z",
                "repo": { "name": "acme/app" },
                "payload": {
                    "ref": "refs/heads/main",
                    "head": "bbb222",
                    "before": "aaa111"
                }
            },
            {
                "type": "PushEvent",
                "created_at": "2026-10-17T09:00:00Z",
                "repo": { "name": "acme/app" },
                "payload": {
                    "ref": "refs/heads/main",
                    "head": "bbb222",
                    "before": "000000"
                }
            },
            {
                "type": "PushEvent",
                "created_at": "2026-10-16T09:00:00Z",
                "repo": { "name": "acme/app" },
                "payload": {
                    "ref": "refs/heads/feature",
                    "head": "ccc333",
                    "before": "bbb222"
                }
            }
        ]);
        let refs = push_head_refs_from_events(events.as_array().unwrap());
        assert_eq!(
            refs,
            vec![
                PushHeadRef {
                    repo_full_name: "acme/app".into(),
                    sha: "bbb222".into(),
                },
                PushHeadRef {
                    repo_full_name: "acme/app".into(),
                    sha: "ccc333".into(),
                },
            ]
        );
    }

    #[test]
    fn parse_repo_commit_uses_html_url() {
        let body = json!({
            "sha": "abcdef0123456789",
            "html_url": "https://company.ghe.com/acme/app/commit/abcdef0123456789",
            "commit": {
                "message": "Enterprise commit\n\nDetails",
                "author": { "date": "2026-10-15T09:30:00Z" }
            }
        });
        let c = parse_repo_commit(&body, "acme/app").unwrap();
        assert_eq!(c.sha, "abcdef0123456789");
        assert_eq!(c.message, "Enterprise commit");
        assert_eq!(c.repo_full_name, "acme/app");
        assert_eq!(c.committed_at, "2026-10-15T09:30:00Z");
        assert_eq!(
            c.url,
            "https://company.ghe.com/acme/app/commit/abcdef0123456789"
        );
    }

    #[test]
    fn summarize_empty_reviews() {
        assert_eq!(summarize_reviews(None), "No reviews");
        assert_eq!(summarize_reviews(Some(&json!([]))), "No reviews");
    }

    #[test]
    fn parse_pr_missing_checks_is_empty_status() {
        let node = serde_json::json!({
            "id": "PR_kwDO_docs",
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
            "reviewThreads": {
                "nodes": [],
                "pageInfo": { "hasNextPage": false, "endCursor": null }
            },
            "latestReviews": { "nodes": [] },
            "reviewRequests": { "nodes": [] }
        });
        let pr = parse_pr_node(&node).unwrap();
        assert_eq!(pr.check_status, "");
        assert_eq!(pr.unresolved_comment_count, 0);
        assert_eq!(pr.reviews_summary, "No reviews");
    }

    #[test]
    fn parse_pr_detail_fixture() {
        let node = json!({
            "number": 42,
            "title": "Expand PR cards",
            "url": "https://github.com/acme/app/pull/42",
            "createdAt": "2026-11-01T10:00:00Z",
            "author": { "login": "alice" },
            "commits": {
                "nodes": [{
                    "commit": {
                        "statusCheckRollup": {
                            "state": "FAILURE",
                            "contexts": {
                                "nodes": [
                                    {
                                        "__typename": "CheckRun",
                                        "name": "CI",
                                        "status": "COMPLETED",
                                        "conclusion": "SUCCESS",
                                        "detailsUrl": "https://example.com/ci"
                                    },
                                    {
                                        "__typename": "CheckRun",
                                        "name": "Lint",
                                        "status": "COMPLETED",
                                        "conclusion": "FAILURE",
                                        "detailsUrl": "https://example.com/lint"
                                    },
                                    {
                                        "__typename": "StatusContext",
                                        "context": "coverage",
                                        "state": "PENDING",
                                        "description": "waiting",
                                        "targetUrl": "https://example.com/cov"
                                    }
                                ]
                            }
                        }
                    }
                }]
            },
            "latestReviews": {
                "nodes": [
                    {
                        "state": "COMMENTED",
                        "submittedAt": "2026-11-02T09:00:00Z",
                        "author": { "login": "bob" }
                    },
                    {
                        "state": "APPROVED",
                        "submittedAt": "2026-11-02T11:00:00Z",
                        "author": { "login": "bob" }
                    },
                    {
                        "state": "CHANGES_REQUESTED",
                        "submittedAt": "2026-11-02T10:00:00Z",
                        "author": { "login": "carol" }
                    }
                ]
            },
            "reviewRequests": {
                "nodes": [
                    { "requestedReviewer": { "login": "dave" } }
                ]
            },
            "reviewThreads": {
                "nodes": [
                    {
                        "isResolved": true,
                        "path": "src/a.rs",
                        "line": 1,
                        "comments": {
                            "nodes": [{
                                "body": "resolved",
                                "createdAt": "2026-11-02T08:00:00Z",
                                "author": { "login": "bob" }
                            }]
                        }
                    },
                    {
                        "isResolved": false,
                        "path": "src/b.rs",
                        "line": 12,
                        "comments": {
                            "nodes": [{
                                "body": "Please fix this",
                                "createdAt": "2026-11-02T09:30:00Z",
                                "author": { "login": "carol" }
                            }]
                        }
                    }
                ],
                "pageInfo": { "hasNextPage": false, "endCursor": null }
            },
            "timelineItems": {
                "nodes": [
                    {
                        "__typename": "PullRequestCommit",
                        "commit": {
                            "oid": "abcdef0123456789",
                            "messageHeadline": "Add expand",
                            "committedDate": "2026-11-02T08:00:00Z",
                            "url": "https://github.com/acme/app/commit/abcdef0123456789"
                        }
                    },
                    {
                        "__typename": "IssueComment",
                        "author": { "login": "bob" },
                        "body": "LGTM after fixes",
                        "createdAt": "2026-11-02T12:00:00Z",
                        "url": "https://github.com/acme/app/pull/42#issuecomment-1"
                    },
                    {
                        "__typename": "ClosedEvent",
                        "createdAt": "2026-11-03T01:00:00Z",
                        "actor": { "login": "alice" }
                    }
                ]
            }
        });
        let detail = parse_pr_detail(&node, "acme/app").unwrap();
        assert_eq!(detail.number, 42);
        assert_eq!(detail.repo_full_name, "acme/app");
        assert_eq!(detail.checks.len(), 3);
        assert_eq!(detail.checks[0].state, "SUCCESS");
        assert_eq!(detail.checks[1].state, "FAILURE");
        assert_eq!(detail.checks[2].name, "coverage");
        assert_eq!(detail.checks[2].state, "PENDING");
        // Latest review per author: bob approved, carol changes requested.
        assert_eq!(detail.reviews.len(), 2);
        let bob = detail.reviews.iter().find(|r| r.author == "bob").unwrap();
        assert_eq!(bob.state, "APPROVED");
        assert_eq!(detail.pending_reviewers, vec!["dave".to_string()]);
        assert_eq!(detail.open_threads.len(), 1);
        assert_eq!(detail.open_threads[0].path, "src/b.rs");
        assert_eq!(detail.open_threads[0].line, Some(12));
        // Synthetic Opened + 3 timeline items, newest first.
        assert_eq!(detail.timeline.len(), 4);
        assert_eq!(detail.timeline[0].kind, GithubPrTimelineKind::Closed);
        assert!(detail
            .timeline
            .iter()
            .any(|t| t.kind == GithubPrTimelineKind::Opened));
        assert!(detail
            .timeline
            .iter()
            .any(|t| t.kind == GithubPrTimelineKind::Commit
                && t.summary.starts_with("abcdef0")));
    }

    #[test]
    fn truncate_body_respects_max() {
        let short = truncate_body("  hello  ");
        assert_eq!(short, "hello");
        let long: String = (0..BODY_PREVIEW_MAX + 20).map(|_| 'x').collect();
        let trimmed = truncate_body(&long);
        assert!(trimmed.ends_with('…'));
        assert_eq!(trimmed.chars().count(), BODY_PREVIEW_MAX + 1);
    }
}
