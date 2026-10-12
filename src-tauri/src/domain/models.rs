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
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
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
        }
    }
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTaskInput {
    pub name: String,
    pub color: Option<String>,
    pub description: Option<String>,
    pub notes: Option<String>,
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
