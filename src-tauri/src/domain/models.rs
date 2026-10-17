use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Task {
    pub id: String,
    pub name: String,
    pub color: String,
    pub description: String,
    pub notes: String,
    pub archived: bool,
    pub starred: bool,
    /// Jira Cloud issue key (e.g. THING-123); empty when unlinked.
    pub jira_issue_key: String,
    /// Milliseconds successfully posted to Jira for this task's current issue key.
    pub jira_reported_ms: i64,
    /// In-flight worklog amount (ms) written before POST; reconciled on next sync.
    #[serde(default)]
    pub jira_pending_ms: i64,
    /// Task total ms when the current issue key was linked; TimeTrove posted =
    /// `max(0, jira_reported_ms - jira_link_baseline_ms)`.
    #[serde(default)]
    pub jira_link_baseline_ms: i64,
    /// Cached Jira issue fields for the current key (refreshed on select / Add).
    #[serde(default)]
    pub jira_issue_summary: String,
    #[serde(default)]
    pub jira_issue_status: String,
    #[serde(default)]
    pub jira_issue_assignee: String,
    #[serde(default)]
    pub jira_issue_estimate_s: Option<i64>,
    #[serde(default)]
    pub jira_issue_total_spent_s: Option<i64>,
    #[serde(default)]
    pub jira_issue_fetched_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Task {
    /// Milliseconds TimeTrove has posted to the current issue while linked.
    pub fn jira_timetrove_posted_ms(&self) -> i64 {
        (self.jira_reported_ms - self.jira_link_baseline_ms).max(0)
    }
}

/// Live / cached details for a Jira Cloud issue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct JiraIssueDetails {
    pub issue_key: String,
    pub summary: String,
    pub status: String,
    pub assignee: String,
    /// Original estimate in seconds, if set on the issue.
    pub estimate_seconds: Option<i64>,
    /// Aggregate work logged on the issue in Jira (all authors), seconds.
    pub total_spent_seconds: Option<i64>,
}

/// Assigned open issue enriched with optional TimeTrove task link.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraDashboardIssue {
    pub issue_key: String,
    pub summary: String,
    pub status: String,
    pub assignee: String,
    pub estimate_seconds: Option<i64>,
    pub total_spent_seconds: Option<i64>,
    /// `{site}/browse/{KEY}` when site URL is configured.
    pub browse_url: String,
    pub task_id: Option<String>,
    pub task_name: Option<String>,
    /// TimeTrove booked ms for this key (`reported − baseline`); null when unlinked.
    pub timetrove_booked_ms: Option<i64>,
}

/// Result of listing assigned open Jira issues (cached).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JiraAssignedIssuesResult {
    pub issues: Vec<JiraDashboardIssue>,
    pub fetched_at: Option<DateTime<Utc>>,
    pub truncated: bool,
    pub from_cache: bool,
    /// Soft error / nudge (e.g. disabled or incomplete credentials).
    pub error: Option<String>,
}

/// Prior Jira issue linked to a task (one active key; history for display only).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskJiraHistoryEntry {
    pub id: String,
    pub task_id: String,
    pub issue_key: String,
    pub title: String,
    /// TimeTrove-posted ms while this key was linked.
    pub booked_ms: i64,
    pub unlinked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeSlice {
    pub id: String,
    pub task_id: String,
    pub start: DateTime<Utc>,
    pub end: Option<DateTime<Utc>>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscardLogEntry {
    pub id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    pub duration_ms: i64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub idle_threshold_mins: u32,
    pub daily_goal_mins: u32,
    pub switch_shortcut: String,
    pub autostart: bool,
    /// "dark" (default) or "light"
    pub theme: String,
    /// Keep the tracker window above other windows (default on for taskbar dock).
    pub tracker_always_on_top: bool,
    /// Whether the tracker bar is shown (persists hide/close across restarts).
    pub tracker_visible: bool,
    /// Show live HH:MM:SS on the tracker bar (default HH:MM).
    pub tracker_show_seconds: bool,
    /// Optional Jira Cloud worklog sync (off by default).
    pub jira_enabled: bool,
    pub jira_site_url: String,
    pub jira_email: String,
    pub jira_api_token: String,
    /// Optional HTTP CONNECT proxy for Jira requests (`http://host:port`). Empty = none.
    #[serde(default)]
    pub jira_proxy_url: String,
    /// Last sync/connection error (read-only for Settings UI; not cleared by save).
    #[serde(default)]
    pub jira_last_error: Option<String>,
    /// ISO timestamp of last successful worklog post.
    #[serde(default)]
    pub jira_last_synced_at: Option<String>,
    /// Optional GitHub Copilot usage tracking (off by default).
    #[serde(default)]
    pub copilot_enabled: bool,
    /// GitHub host: `github.com` or Enterprise Cloud host (e.g. `company.ghe.com`).
    #[serde(default = "default_copilot_github_host")]
    pub copilot_github_host: String,
    /// Optional GitHub username; auto-filled from the token via `/user` when empty.
    #[serde(default)]
    pub copilot_username: String,
    /// GitHub token (fine-grained PAT with Copilot Requests, or OAuth/`gh` token).
    #[serde(default)]
    pub copilot_api_token: String,
    /// Optional HTTP CONNECT proxy for GitHub requests (`http://host:port`). Empty = none.
    #[serde(default)]
    pub copilot_proxy_url: String,
    /// Poll interval in minutes (default 30).
    #[serde(default = "default_copilot_poll_interval_mins")]
    pub copilot_poll_interval_mins: u32,
    /// Manual monthly allowance when the API has no usable entitlement.
    #[serde(default)]
    pub copilot_monthly_limit_override: Option<f64>,
    /// Last sync/connection error (read-only for Settings UI; not cleared by save).
    #[serde(default)]
    pub copilot_last_error: Option<String>,
    /// ISO timestamp of last successful Copilot usage poll.
    #[serde(default)]
    pub copilot_last_synced_at: Option<String>,
    /// Optional GitHub activity page (open PRs + recent commits; off by default).
    #[serde(default)]
    pub github_activity_enabled: bool,
    /// Poll interval in minutes for GitHub activity (default 30).
    #[serde(default = "default_github_activity_poll_interval_mins")]
    pub github_activity_poll_interval_mins: u32,
    /// Last GitHub activity sync/connection error.
    #[serde(default)]
    pub github_activity_last_error: Option<String>,
    /// ISO timestamp of last successful GitHub activity poll.
    #[serde(default)]
    pub github_activity_last_synced_at: Option<String>,
}

fn default_copilot_github_host() -> String {
    "github.com".into()
}

fn default_copilot_poll_interval_mins() -> u32 {
    30
}

fn default_github_activity_poll_interval_mins() -> u32 {
    30
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_threshold_mins: 10,
            daily_goal_mins: 480,
            switch_shortcut: "CommandOrControl+Shift+T".to_string(),
            autostart: true,
            theme: "dark".to_string(),
            tracker_always_on_top: true,
            tracker_visible: true,
            tracker_show_seconds: false,
            jira_enabled: false,
            jira_site_url: String::new(),
            jira_email: String::new(),
            jira_api_token: String::new(),
            jira_proxy_url: String::new(),
            jira_last_error: None,
            jira_last_synced_at: None,
            copilot_enabled: false,
            copilot_github_host: default_copilot_github_host(),
            copilot_username: String::new(),
            copilot_api_token: String::new(),
            copilot_proxy_url: String::new(),
            copilot_poll_interval_mins: default_copilot_poll_interval_mins(),
            copilot_monthly_limit_override: None,
            copilot_last_error: None,
            copilot_last_synced_at: None,
            github_activity_enabled: false,
            github_activity_poll_interval_mins: default_github_activity_poll_interval_mins(),
            github_activity_last_error: None,
            github_activity_last_synced_at: None,
        }
    }
}

/// One quota pool from the Copilot internal API (premium, chat, completions, …).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CopilotQuotaPool {
    pub id: String,
    pub entitlement: f64,
    pub remaining: f64,
    pub used: f64,
    pub percent_remaining: f64,
    pub percent_used: f64,
    pub unlimited: bool,
    pub overage_count: f64,
    pub overage_permitted: bool,
}

/// Normalized Copilot usage for UI / tracker (latest successful poll + overrides).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotUsageSummary {
    pub plan: String,
    pub username: String,
    pub unit: String,
    pub entitlement: f64,
    pub used: f64,
    pub remaining: f64,
    pub percent_used: f64,
    pub unlimited: bool,
    pub overage_count: f64,
    pub overage_permitted: bool,
    pub reset_date: Option<String>,
    pub pools: Vec<CopilotQuotaPool>,
    pub limit_source: String,
    pub fetched_at: Option<String>,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

/// Historical poll row for charts.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotUsageHistoryPoint {
    pub id: String,
    pub fetched_at: DateTime<Utc>,
    pub plan: String,
    pub entitlement: f64,
    pub used: f64,
    pub remaining: f64,
    pub percent_used: f64,
    pub unit: String,
}

/// Open PR summary for the GitHub activity page.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubPrSummary {
    pub number: i32,
    pub title: String,
    pub url: String,
    pub repo_full_name: String,
    pub created_at: String,
    /// SUCCESS / FAILURE / PENDING / EXPECTED / ERROR / …
    pub check_status: String,
    /// APPROVED / CHANGES_REQUESTED / REVIEW_REQUIRED / …
    pub review_decision: String,
    pub unresolved_comment_count: u32,
    /// Short human summary of review states.
    pub reviews_summary: String,
    /// True when the viewer still needs to submit a review.
    pub needs_my_review: bool,
    /// Internal: logins currently requested as reviewers (not sent to UI).
    #[serde(default, skip_serializing)]
    pub requested_reviewer_logins: Vec<String>,
}

/// Recent commit by the authenticated user.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubCommitSummary {
    pub sha: String,
    pub message: String,
    pub repo_full_name: String,
    pub committed_at: String,
    pub url: String,
}

/// Cached digest for the GitHub activity tab.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GithubActivityDigest {
    pub authored_prs: Vec<GithubPrSummary>,
    pub review_prs: Vec<GithubPrSummary>,
    pub recent_commits: Vec<GithubCommitSummary>,
    pub username: String,
    pub fetched_at: Option<String>,
    pub last_synced_at: Option<String>,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum EnginePhase {
    Stopped,
    Tracking {
        #[serde(rename = "taskId")]
        task_id: String,
        #[serde(rename = "sliceId")]
        slice_id: String,
    },
    NeedsAttribution {
        #[serde(rename = "gapStart")]
        gap_start: DateTime<Utc>,
        #[serde(rename = "previousTaskId")]
        previous_task_id: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrackerState {
    pub phase: EnginePhase,
    pub current_task: Option<Task>,
    pub previous_task: Option<Task>,
    pub daily_total_ms: i64,
    pub daily_total_display: String,
    pub goal_met: bool,
    pub daily_goal_mins: u32,
    pub gap_duration_ms: Option<i64>,
    pub gap_band: Option<String>,
    pub db_error: Option<String>,
    pub tracking: bool,
    /// When set and in the future, gap prompts are suppressed while tracking continues.
    pub snooze_until: Option<DateTime<Utc>>,
    /// Note on the running slice when tracking; empty otherwise.
    pub current_slice_note: String,
    /// Copilot usage summary when the feature is enabled; null otherwise.
    #[serde(default)]
    pub copilot_usage: Option<CopilotUsageSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    #[serde(default)]
    pub jira_issue_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateTaskInput {
    pub id: String,
    pub name: Option<String>,
    pub color: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
    pub archived: Option<bool>,
    pub starred: Option<bool>,
    #[serde(default)]
    pub jira_issue_key: Option<String>,
    /// Staged issue details from Add; applied when the issue key changes on Save.
    #[serde(default)]
    pub jira_issue_details: Option<JiraIssueDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TaskBookingSummary {
    pub this_week_ms: i64,
    pub this_week_display: String,
    pub this_week_start: String,
    pub last_week_ms: i64,
    pub last_week_display: String,
    pub last_week_start: String,
    pub last_booked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimesheetCell {
    pub task_id: String,
    pub day: String,
    pub duration_ms: i64,
    pub display: String,
    /// Combined notes from slices overlapping this task-day (` · ` joined).
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimesheetRow {
    pub task: Task,
    pub cells: Vec<TimesheetCell>,
    pub total_ms: i64,
    pub total_display: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Timesheet {
    pub week_start: String,
    pub days: Vec<String>,
    pub rows: Vec<TimesheetRow>,
    pub day_totals_ms: Vec<i64>,
    pub day_totals_display: Vec<String>,
    pub grand_total_ms: i64,
    pub grand_total_display: String,
    pub csv: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsTaskTotal {
    pub task_id: String,
    pub name: String,
    pub color: String,
    pub total_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsWeekTotal {
    pub week_start: String,
    pub total_ms: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InsightsReport {
    pub range_start: String,
    pub range_end: String,
    pub total_ms: i64,
    pub max_day_ms: i64,
    /// Mean daily total over qualifying days (≥2h); null if none.
    pub avg_day_ms: Option<i64>,
    /// Mon=0 … Sun=6; null entry when no qualifying days on that weekday.
    pub avg_by_weekday_ms: Vec<Option<i64>>,
    pub earliest_start_mins: Option<u32>,
    pub latest_finish_mins: Option<u32>,
    pub avg_start_mins: Option<u32>,
    pub avg_finish_mins: Option<u32>,
    pub avg_lunch_ms: Option<i64>,
    pub qualifying_days: u32,
    /// 0.0–1.0; null when no qualifying days.
    pub goal_hit_rate: Option<f64>,
    pub avg_session_ms: Option<i64>,
    pub discarded_ms: i64,
    pub tasks: Vec<InsightsTaskTotal>,
    pub weekly: Vec<InsightsWeekTotal>,
    pub by_hour_ms: Vec<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSliceInput {
    pub task_id: String,
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSliceInput {
    pub id: String,
    pub task_id: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitSliceInput {
    pub id: String,
    pub at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn settings_defaults() {
        let s = Settings::default();
        assert_eq!(s.idle_threshold_mins, 10);
        assert_eq!(s.daily_goal_mins, 480);
        assert_eq!(s.theme, "dark");
        assert!(s.autostart);
        assert!(s.tracker_always_on_top);
        assert!(s.tracker_visible);
        assert!(!s.tracker_show_seconds);
        assert!(!s.jira_enabled);
        assert!(s.jira_site_url.is_empty());
        assert!(s.jira_email.is_empty());
        assert!(s.jira_api_token.is_empty());
        assert!(s.jira_proxy_url.is_empty());
        assert!(s.jira_last_error.is_none());
        assert!(s.jira_last_synced_at.is_none());
        assert!(!s.copilot_enabled);
        assert_eq!(s.copilot_github_host, "github.com");
        assert_eq!(s.copilot_poll_interval_mins, 30);
        assert!(s.copilot_api_token.is_empty());
        assert!(s.copilot_proxy_url.is_empty());
        assert!(s.copilot_monthly_limit_override.is_none());
        assert!(!s.github_activity_enabled);
        assert_eq!(s.github_activity_poll_interval_mins, 30);
        assert!(s.github_activity_last_error.is_none());
        assert!(s.github_activity_last_synced_at.is_none());
    }

    #[test]
    fn engine_phase_serde_tags() {
        let stopped = serde_json::to_value(EnginePhase::Stopped).unwrap();
        assert_eq!(stopped["kind"], "stopped");

        let tracking = serde_json::to_value(EnginePhase::Tracking {
            task_id: "t1".into(),
            slice_id: "s1".into(),
        })
        .unwrap();
        assert_eq!(tracking["kind"], "tracking");
        assert_eq!(tracking["taskId"], "t1");
        assert_eq!(tracking["sliceId"], "s1");

        let gap_start = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let needs = serde_json::to_value(EnginePhase::NeedsAttribution {
            gap_start,
            previous_task_id: "t2".into(),
        })
        .unwrap();
        assert_eq!(needs["kind"], "needsAttribution");
        assert_eq!(needs["previousTaskId"], "t2");
        assert!(needs["gapStart"].as_str().unwrap().contains("2026-01-01"));
    }
}
