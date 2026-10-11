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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            idle_threshold_mins: 10,
            daily_goal_mins: 480,
            switch_shortcut: "CommandOrControl+Shift+T".to_string(),
            autostart: true,
            theme: "dark".to_string(),
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TimesheetCell {
    pub task_id: String,
    pub day: String,
    pub duration_ms: i64,
    pub display: String,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSliceInput {
    pub id: String,
    pub task_id: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitSliceInput {
    pub id: String,
    pub at: DateTime<Utc>,
}
