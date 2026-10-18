import { invoke } from "@tauri-apps/api/core";
import { waitForEngineReady, withStartupRetry } from "./startupRetry";
import type {
  CopilotUsageHistoryPoint,
  CopilotUsageSummary,
  GithubActivityDigest,
  InsightsReport,
  JiraAssignedIssuesResult,
  JiraIssueDetails,
  Settings,
  Task,
  TaskBookingSummary,
  TaskJiraHistoryEntry,
  TimeSlice,
  Timesheet,
  TrackerState,
} from "./types";

/** Wait for manage(), then retry while setup is still registering State. */
async function invokeManaged<T>(
  cmd: string,
  args?: Record<string, unknown>,
): Promise<T> {
  await waitForEngineReady();
  return withStartupRetry(() => invoke<T>(cmd, args));
}

export const api = {
  getTrackerState: () => invokeManaged<TrackerState>("get_tracker_state"),
  listTasks: (includeArchived = true) =>
    invokeManaged<Task[]>("list_tasks", { includeArchived }),
  createTask: (input: {
    name: string;
    color?: string;
    description?: string;
    notes?: string;
    jiraIssueKey?: string;
  }) => invokeManaged<Task>("create_task", { input }),
  updateTask: (input: {
    id: string;
    name?: string;
    color?: string;
    description?: string;
    notes?: string;
    archived?: boolean;
    starred?: boolean;
    jiraIssueKey?: string;
    jiraIssueDetails?: JiraIssueDetails;
  }) => invokeManaged<Task>("update_task", { input }),
  startJiraConnectionTest: () =>
    invokeManaged<void>("start_jira_connection_test"),
  fetchJiraIssue: (issueKey: string) =>
    invokeManaged<JiraIssueDetails>("fetch_jira_issue", { issueKey }),
  refreshJiraIssue: (taskId: string) =>
    invokeManaged<JiraIssueDetails | null>("refresh_jira_issue", { taskId }),
  listAssignedJiraIssues: (force = false) =>
    invokeManaged<JiraAssignedIssuesResult>("list_assigned_jira_issues", {
      force,
    }),
  openExternalUrl: (url: string) =>
    invokeManaged<void>("open_external_url", { url }),
  listTaskJiraHistory: (taskId: string) =>
    invokeManaged<TaskJiraHistoryEntry[]>("list_task_jira_history", { taskId }),
  getCopilotUsage: () =>
    invokeManaged<CopilotUsageSummary | null>("get_copilot_usage"),
  getCopilotUsageHistory: (opts?: { limit?: number; since?: string }) =>
    invokeManaged<CopilotUsageHistoryPoint[]>("get_copilot_usage_history", {
      limit: opts?.limit ?? null,
      since: opts?.since ?? null,
    }),
  refreshCopilotUsage: () =>
    invokeManaged<CopilotUsageSummary | null>("refresh_copilot_usage"),
  startCopilotConnectionTest: () =>
    invokeManaged<void>("start_copilot_connection_test"),
  getGithubActivity: () =>
    invokeManaged<GithubActivityDigest | null>("get_github_activity"),
  refreshGithubActivity: () =>
    invokeManaged<GithubActivityDigest | null>("refresh_github_activity"),
  getTaskBookingSummary: (taskId: string) =>
    invokeManaged<TaskBookingSummary>("get_task_booking_summary", { taskId }),
  startTask: (taskId: string) =>
    invokeManaged<TrackerState>("start_task", { taskId }),
  stopTracking: () => invokeManaged<TrackerState>("stop_tracking"),
  switchTask: (taskId: string) =>
    invokeManaged<TrackerState>("switch_task", { taskId }),
  createAndStart: (name: string) =>
    invokeManaged<TrackerState>("create_and_start", { name }),
  resolveGapPrevious: () => invokeManaged<TrackerState>("resolve_gap_previous"),
  resolveGapOther: (taskId: string) =>
    invokeManaged<TrackerState>("resolve_gap_other", { taskId }),
  createAndResolveGapOther: (name: string) =>
    invokeManaged<TrackerState>("create_and_resolve_gap_other", { name }),
  resolveGapDiscard: () => invokeManaged<TrackerState>("resolve_gap_discard"),
  getSettings: () => invokeManaged<Settings>("get_settings"),
  updateSettings: (settings: Settings) =>
    invokeManaged<Settings>("update_settings", { settings }),
  getTimesheet: (weekStart: string) =>
    invokeManaged<Timesheet>("get_timesheet", { weekStart }),
  getInsights: (rangeStart: string, rangeEnd: string) =>
    invokeManaged<InsightsReport>("get_insights", { rangeStart, rangeEnd }),
  listSlicesForDay: (day: string) =>
    invokeManaged<TimeSlice[]>("list_slices_for_day", { day }),
  createSlice: (input: {
    taskId: string;
    start: string;
    end: string;
    note?: string;
  }) => invokeManaged<TimeSlice>("create_slice", { input }),
  updateSlice: (input: {
    id: string;
    taskId?: string;
    start?: string;
    end?: string;
    note?: string;
  }) => invokeManaged<TimeSlice>("update_slice", { input }),
  deleteSlice: (id: string) => invokeManaged<void>("delete_slice", { id }),
  splitSlice: (input: { id: string; at: string }) =>
    invokeManaged<[TimeSlice, TimeSlice]>("split_slice", { input }),
  moveSlice: (id: string, taskId: string) =>
    invokeManaged<TimeSlice>("move_slice", { id, taskId }),
  coalesceNearAdjacent: () => invokeManaged<void>("coalesce_near_adjacent"),
  getDbPath: () => invokeManaged<string>("get_db_path"),
  revealDatabase: () => invokeManaged<void>("reveal_database"),
  exportDatabase: (dest: string) =>
    invokeManaged<void>("export_database", { dest }),
  importDatabase: (source: string) =>
    invokeManaged<void>("import_database", { source }),
  openSwitcher: () => invokeManaged<void>("open_switcher"),
  closeSwitcher: () => invokeManaged<void>("close_switcher"),
  startSnooze: (until: string) =>
    invokeManaged<TrackerState>("start_snooze", { until }),
  clearSnooze: () => invokeManaged<TrackerState>("clear_snooze"),
  openSnoozePicker: () => invokeManaged<void>("open_snooze_picker"),
  closeSnoozePicker: () => invokeManaged<void>("close_snooze_picker"),
  openMainWindow: () => invokeManaged<void>("open_main_window"),
  setTrackerVisible: (visible: boolean) =>
    invokeManaged<void>("set_tracker_visible", { visible }),
  resetTrackerPosition: () => invokeManaged<void>("reset_tracker_position"),
  setTrackerAlwaysOnTop: (alwaysOnTop: boolean) =>
    invokeManaged<Settings>("set_tracker_always_on_top", { alwaysOnTop }),
  currentWeekStart: () => invokeManaged<string>("current_week_start"),
  shiftWeek: (weekStart: string, deltaWeeks: number) =>
    invokeManaged<string>("shift_week", { weekStart, deltaWeeks }),
};
