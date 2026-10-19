use super::http::{self, GithubHttpConfig};
use crate::domain::{GithubActivityDigest, GithubCommitSummary, GithubPrSummary};
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
      commits(last: 1) {
        nodes {
          commit {
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

/// Cap follow-up thread pages so a pathological PR cannot loop forever (100 × 50 = 5000 threads).
const MAX_REVIEW_THREAD_PAGES: usize = 50;
const MAX_RECENT_COMMITS: usize = 20;

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

    let mut diagnostics = Vec::new();
    diagnostics.push(format!("apiBase={base}"));

    // Always bind to the token — Settings username is not used for queries.
    let user = http::fetch_authenticated_user(config)?;
    let login = user.login;
    diagnostics.push(format!("tokenLogin={login}"));
    let hint = username_hint.trim();
    diagnostics.push(if hint.is_empty() {
        "settingsUsername=(blank)".into()
    } else {
        format!("settingsUsername={hint}")
    });
    diagnostics.push(format!(
        "usernameMatch={}",
        username_match_label(hint, &login)
    ));
    match &user.oauth_scopes {
        Some(scopes) => diagnostics.push(format!("oauthScopes={scopes}")),
        None => diagnostics.push("oauthScopes=unavailable".into()),
    }

    let mut authored_prs =
        fetch_viewer_authored_prs(&agent, &base, &config.api_token, &mut diagnostics)?;
    for pr in &mut authored_prs {
        pr.needs_my_review = false;
    }
    authored_prs.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let requested = search_prs(
        &agent,
        &base,
        &config.api_token,
        "is:open is:pr review-requested:@me",
        "reviewRequested",
        &mut diagnostics,
    )?;
    let reviewed = search_prs(
        &agent,
        &base,
        &config.api_token,
        "is:open is:pr reviewed-by:@me",
        "reviewedBy",
        &mut diagnostics,
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
    let needs_me = review_prs.iter().filter(|p| p.needs_my_review).count();
    diagnostics.push(format!("reviewMergedUnique={}", review_prs.len()));
    diagnostics.push(format!("reviewNeedsMe={needs_me}"));

    let recent_commits =
        fetch_recent_commits(&agent, &base, &config.api_token, &login, &mut diagnostics)?;

    diagnostics.push(format!(
        "final authored={} review={} commits={}",
        authored_prs.len(),
        review_prs.len(),
        recent_commits.len()
    ));

    Ok(GithubActivityDigest {
        authored_prs,
        review_prs,
        recent_commits,
        fetched_at: None,
        last_synced_at: None,
        last_error: None,
        username: login,
        diagnostics,
    })
}

/// Compare Settings username hint to token login for diagnostics.
fn username_match_label(settings_username: &str, token_login: &str) -> &'static str {
    let hint = settings_username.trim();
    if hint.is_empty() {
        "blank"
    } else if hint.eq_ignore_ascii_case(token_login) {
        "yes"
    } else {
        "no"
    }
}

fn format_search_diag(
    label: &str,
    issue_count: u64,
    nodes_len: usize,
    null_nodes: usize,
    parsed: usize,
    after_hydrate: usize,
) -> String {
    format!(
        "{label}: issueCount={issue_count} nodesLen={nodes_len} nullNodes={null_nodes} \
         parsed={parsed} afterHydrate={after_hydrate}"
    )
}

fn fetch_viewer_authored_prs(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    diagnostics: &mut Vec<String>,
) -> Result<Vec<GithubPrSummary>, String> {
    let body = http::post_graphql(agent, api_base, token, VIEWER_AUTHORED_PRS_QUERY, json!({}))?;
    let nodes = body
        .pointer("/data/viewer/pullRequests/nodes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "GitHub GraphQL viewer.pullRequests missing nodes".to_string())?;
    let nodes_len = nodes.len();
    let null_nodes = nodes.iter().filter(|n| n.is_null()).count();
    let identities = parse_pr_identity_nodes(nodes)?;
    let parsed = identities.len();
    diagnostics.push(format!(
        "authoredViewerNodes={nodes_len} nullNodes={null_nodes} parsed={parsed}"
    ));
    let prs = hydrate_prs(agent, api_base, token, identities)?;
    diagnostics.push(format!("authoredAfterHydrate={}", prs.len()));
    Ok(prs)
}

fn search_prs(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    query: &str,
    label: &str,
    diagnostics: &mut Vec<String>,
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
    let nodes_len = nodes.len();
    let null_nodes = nodes.iter().filter(|n| n.is_null()).count();
    let identities = search_identities_or_err(issue_count, nodes)?;
    let parsed = identities.len();
    let prs = hydrate_prs(agent, api_base, token, identities)?;
    diagnostics.push(format_search_diag(
        label,
        issue_count,
        nodes_len,
        null_nodes,
        parsed,
        prs.len(),
    ));
    Ok(prs)
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
            requested_reviewer_logins: Vec::new(),
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
    pr.reviews_summary = summarize_reviews(node.pointer("/latestReviews/nodes"));
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
    diagnostics: &mut Vec<String>,
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

    let search_items = items.len();
    diagnostics.push(format!("commitSearchItems={search_items}"));

    let from_search: Vec<GithubCommitSummary> =
        items.iter().filter_map(parse_commit_item).collect();
    if !from_search.is_empty() {
        diagnostics.push("commitsSource=search".into());
        diagnostics.push(format!("commitsHydrated={}", from_search.len()));
        return Ok(from_search);
    }

    // Search often returns empty for fine-grained PATs / SSO; fall back to public events.
    diagnostics.push("commitsSource=events".into());
    fetch_commits_from_events(agent, api_base, token, login, diagnostics)
}

fn fetch_commits_from_events(
    agent: &ureq::Agent,
    api_base: &str,
    token: &str,
    login: &str,
    diagnostics: &mut Vec<String>,
) -> Result<Vec<GithubCommitSummary>, String> {
    let url = format!("{api_base}/users/{login}/events?per_page=100");
    let body = http::get_json(agent, &url, token)?;
    let events = body
        .as_array()
        .ok_or_else(|| "GitHub user events response is not an array".to_string())?;
    // Events API no longer embeds commit summaries (Oct 2025); only head/before SHAs remain.
    let refs = push_head_refs_from_events(events);
    diagnostics.push(format!("eventPushHeads={}", refs.len()));
    let mut out = Vec::with_capacity(refs.len());
    let mut skipped = 0usize;
    for r in refs {
        match fetch_repo_commit(agent, api_base, token, &r.repo_full_name, &r.sha) {
            Ok(Some(commit)) => out.push(commit),
            Ok(None) | Err(_) => {
                // Skip private/inaccessible repos; keep collecting from later events.
                skipped += 1;
                continue;
            }
        }
        if out.len() >= MAX_RECENT_COMMITS {
            break;
        }
    }
    diagnostics.push(format!("commitsHydrated={}", out.len()));
    diagnostics.push(format!("commitsSkipped={skipped}"));
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
    fn username_match_labels() {
        assert_eq!(username_match_label("", "octocat"), "blank");
        assert_eq!(username_match_label("  ", "octocat"), "blank");
        assert_eq!(username_match_label("Octocat", "octocat"), "yes");
        assert_eq!(username_match_label("other", "octocat"), "no");
    }

    #[test]
    fn search_diag_line_format() {
        let line = format_search_diag("reviewRequested", 3, 3, 1, 2, 2);
        assert!(line.contains("reviewRequested:"));
        assert!(line.contains("issueCount=3"));
        assert!(line.contains("nodesLen=3"));
        assert!(line.contains("nullNodes=1"));
        assert!(line.contains("parsed=2"));
        assert!(line.contains("afterHydrate=2"));
    }

    #[test]
    fn digest_diagnostics_default_deserializes() {
        let json = r#"{
            "authoredPrs": [],
            "reviewPrs": [],
            "recentCommits": [],
            "username": "octocat",
            "fetchedAt": null,
            "lastSyncedAt": null,
            "lastError": null
        }"#;
        let d: GithubActivityDigest = serde_json::from_str(json).unwrap();
        assert!(d.diagnostics.is_empty());
        assert_eq!(d.username, "octocat");
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
}
