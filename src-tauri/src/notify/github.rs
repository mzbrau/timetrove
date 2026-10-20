use super::{click_for_items, format_item_body, get_setting_string, set_setting_string, show};
use crate::db::Db;
use crate::domain::{GithubActivityDigest, GithubPrSummary};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::AppHandle;

const KEY_SNAPSHOT: &str = "github_notify_snapshot";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PrSnap {
    pub check_status: String,
    pub review_decision: String,
    /// Fingerprint of latest review node IDs (not the display summary).
    #[serde(default)]
    pub reviews_fingerprint: String,
    pub needs_my_review: bool,
    pub label: String,
    pub url: String,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubNotifySnapshot {
    /// Keyed by PR url.
    pub prs: HashMap<String, PrSnap>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubChangeCategory {
    pub title: String,
    pub labels: Vec<String>,
    pub urls: Vec<String>,
}

fn pr_label(pr: &GithubPrSummary) -> String {
    format!("{}#{} — {}", pr.repo_full_name, pr.number, pr.title)
}

fn snap_from_pr(pr: &GithubPrSummary) -> PrSnap {
    PrSnap {
        check_status: pr.check_status.clone(),
        review_decision: pr.review_decision.clone(),
        reviews_fingerprint: pr.reviews_fingerprint.clone(),
        needs_my_review: pr.needs_my_review,
        label: pr_label(pr),
        url: pr.url.clone(),
    }
}

pub fn snapshot_from_digest(digest: &GithubActivityDigest) -> GithubNotifySnapshot {
    let mut prs = HashMap::new();
    for pr in &digest.authored_prs {
        prs.insert(pr.url.clone(), snap_from_pr(pr));
    }
    for pr in &digest.review_prs {
        prs.entry(pr.url.clone())
            .and_modify(|s| {
                // Prefer true if either list says the viewer still needs to review.
                if pr.needs_my_review {
                    s.needs_my_review = true;
                }
                if !pr.reviews_fingerprint.is_empty() {
                    s.reviews_fingerprint = pr.reviews_fingerprint.clone();
                }
                if !pr.check_status.is_empty() {
                    s.check_status = pr.check_status.clone();
                }
                if !pr.review_decision.is_empty() {
                    s.review_decision = pr.review_decision.clone();
                }
            })
            .or_insert_with(|| snap_from_pr(pr));
    }
    GithubNotifySnapshot { prs }
}

/// Diff previous vs current snapshot into per-category change lists.
pub fn diff_github(
    prev: &GithubNotifySnapshot,
    next: &GithubNotifySnapshot,
) -> Vec<GithubChangeCategory> {
    let mut check: Vec<(String, String)> = Vec::new();
    let mut review_decision: Vec<(String, String)> = Vec::new();
    let mut reviews: Vec<(String, String)> = Vec::new();
    let mut appeared: Vec<(String, String)> = Vec::new();

    for (url, cur) in &next.prs {
        match prev.prs.get(url) {
            None => {
                if cur.needs_my_review {
                    appeared.push((cur.label.clone(), url.clone()));
                }
            }
            Some(old) => {
                if old.check_status != cur.check_status {
                    check.push((cur.label.clone(), url.clone()));
                }
                if old.review_decision != cur.review_decision {
                    review_decision.push((cur.label.clone(), url.clone()));
                }
                if old.reviews_fingerprint != cur.reviews_fingerprint
                    && !cur.reviews_fingerprint.is_empty()
                {
                    reviews.push((cur.label.clone(), url.clone()));
                }
                if !old.needs_my_review && cur.needs_my_review {
                    appeared.push((cur.label.clone(), url.clone()));
                }
            }
        }
    }

    let mut out = Vec::new();
    let push = |out: &mut Vec<GithubChangeCategory>, title: &str, items: Vec<(String, String)>| {
        if items.is_empty() {
            return;
        }
        let (labels, urls): (Vec<_>, Vec<_>) = items.into_iter().unzip();
        out.push(GithubChangeCategory {
            title: title.into(),
            labels,
            urls,
        });
    };
    push(&mut out, "PR checks updated", check);
    push(&mut out, "PR review decision changed", review_decision);
    push(&mut out, "PR reviews updated", reviews);
    push(&mut out, "New PRs awaiting your review", appeared);
    out
}

pub fn clear_snapshot(db: &Db) {
    let _ = set_setting_string(db, KEY_SNAPSHOT, "");
}

pub fn check_github_activity(app: &AppHandle, db: &Db, digest: &GithubActivityDigest) {
    let Ok(settings) = db.get_settings() else {
        return;
    };
    if !settings.notify_github_pr || !settings.github_activity_enabled {
        return;
    }

    let next = snapshot_from_digest(digest);
    let prev_json = get_setting_string(db, KEY_SNAPSHOT)
        .ok()
        .flatten()
        .unwrap_or_default();
    let baseline = prev_json.is_empty();
    let prev: GithubNotifySnapshot = if baseline {
        GithubNotifySnapshot::default()
    } else {
        serde_json::from_str(&prev_json).unwrap_or_default()
    };

    if !baseline {
        for cat in diff_github(&prev, &next) {
            let body = format_item_body(&cat.labels);
            let action = click_for_items(&cat.urls, "github");
            show(app, &cat.title, &body, action, super::NotifyKind::Github);
        }
    }

    if let Ok(json) = serde_json::to_string(&next) {
        let _ = set_setting_string(db, KEY_SNAPSHOT, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(url: &str, check: &str, decision: &str, fingerprint: &str, needs: bool) -> PrSnap {
        PrSnap {
            check_status: check.into(),
            review_decision: decision.into(),
            reviews_fingerprint: fingerprint.into(),
            needs_my_review: needs,
            label: format!("label-{url}"),
            url: url.into(),
        }
    }

    #[test]
    fn aggregates_check_changes() {
        let mut prev = GithubNotifySnapshot::default();
        prev.prs.insert(
            "u1".into(),
            pr("u1", "PENDING", "REVIEW_REQUIRED", "", false),
        );
        prev.prs.insert(
            "u2".into(),
            pr("u2", "PENDING", "REVIEW_REQUIRED", "", false),
        );
        let mut next = prev.clone();
        next.prs.get_mut("u1").unwrap().check_status = "SUCCESS".into();
        next.prs.get_mut("u2").unwrap().check_status = "FAILURE".into();
        let cats = diff_github(&prev, &next);
        assert_eq!(cats.len(), 1);
        assert_eq!(cats[0].title, "PR checks updated");
        assert_eq!(cats[0].urls.len(), 2);
    }

    #[test]
    fn detects_needs_my_review_transition() {
        let mut prev = GithubNotifySnapshot::default();
        prev.prs.insert(
            "u1".into(),
            pr("u1", "PENDING", "REVIEW_REQUIRED", "r1", false),
        );
        let mut next = GithubNotifySnapshot::default();
        next.prs.insert(
            "u1".into(),
            pr("u1", "PENDING", "REVIEW_REQUIRED", "r1", true),
        );
        next.prs.insert(
            "u2".into(),
            pr("u2", "PENDING", "REVIEW_REQUIRED", "r2", true),
        );
        let cats = diff_github(&prev, &next);
        let appeared = cats
            .iter()
            .find(|c| c.title.contains("awaiting"))
            .expect("appeared category");
        assert_eq!(appeared.urls.len(), 2);
    }

    #[test]
    fn review_fingerprint_change() {
        let mut prev = GithubNotifySnapshot::default();
        prev.prs.insert(
            "u1".into(),
            pr("u1", "SUCCESS", "REVIEW_REQUIRED", "id-a", false),
        );
        let mut next = prev.clone();
        next.prs.get_mut("u1").unwrap().reviews_fingerprint = "id-b".into();
        let cats = diff_github(&prev, &next);
        assert_eq!(cats.len(), 1);
        assert!(cats[0].title.contains("reviews"));
    }
}
