export interface Task {
  id: string;
  name: string;
  color: string;
  description: string;
  notes: string;
  archived: boolean;
  starred: boolean;
  /** Jira Cloud issue key (e.g. THING-123); empty when unlinked. */
  jiraIssueKey: string;
  /** Milliseconds successfully posted to Jira for this task's current issue key. */
  jiraReportedMs: number;
  /** In-flight worklog ms (internal); not shown in UI. */
  jiraPendingMs: number;
  /** Task total ms when the current issue was linked. */
  jiraLinkBaselineMs: number;
  /** Cached Jira fields for the current linked issue. */
  jiraIssueSummary: string;
  jiraIssueStatus: string;
  jiraIssueAssignee: string;
  jiraIssueEstimateS: number | null;
  jiraIssueTotalSpentS: number | null;
  jiraIssueFetchedAt: string | null;
  createdAt: string;
  updatedAt: string;
}

export interface JiraIssueDetails {
  issueKey: string;
  summary: string;
  status: string;
  assignee: string;
  estimateSeconds: number | null;
  totalSpentSeconds: number | null;
}

export interface JiraDashboardIssue {
  issueKey: string;
  summary: string;
  status: string;
  assignee: string;
  estimateSeconds: number | null;
  totalSpentSeconds: number | null;
  browseUrl: string;
  taskId: string | null;
  taskName: string | null;
  /** TimeTrove booked ms for this key; null when unlinked. */
  timetroveBookedMs: number | null;
}

export interface JiraAssignedIssuesResult {
  issues: JiraDashboardIssue[];
  fetchedAt: string | null;
  truncated: boolean;
  fromCache: boolean;
  error: string | null;
}

export interface TaskJiraHistoryEntry {
  id: string;
  taskId: string;
  issueKey: string;
  title: string;
  bookedMs: number;
  unlinkedAt: string;
}

export interface TaskBookingSummary {
  thisWeekMs: number;
  thisWeekDisplay: string;
  thisWeekStart: string;
  lastWeekMs: number;
  lastWeekDisplay: string;
  lastWeekStart: string;
  lastBookedAt: string | null;
}

export type EnginePhase =
  | { kind: "stopped" }
  | { kind: "tracking"; taskId: string; sliceId: string }
  | { kind: "needsAttribution"; gapStart: string; previousTaskId: string };

export interface CopilotQuotaPool {
  id: string;
  entitlement: number;
  remaining: number;
  used: number;
  percentRemaining: number;
  percentUsed: number;
  unlimited: boolean;
  overageCount: number;
  overagePermitted: boolean;
}

export interface CopilotUsageSummary {
  plan: string;
  username: string;
  unit: string;
  entitlement: number;
  used: number;
  remaining: number;
  percentUsed: number;
  unlimited: boolean;
  overageCount: number;
  overagePermitted: boolean;
  resetDate: string | null;
  pools: CopilotQuotaPool[];
  limitSource: string;
  fetchedAt: string | null;
  lastSyncedAt: string | null;
  lastError: string | null;
}

export interface CopilotUsageHistoryPoint {
  id: string;
  fetchedAt: string;
  plan: string;
  entitlement: number;
  used: number;
  remaining: number;
  percentUsed: number;
  unit: string;
}

export interface TrackerState {
  phase: EnginePhase;
  currentTask: Task | null;
  previousTask: Task | null;
  dailyTotalMs: number;
  dailyTotalDisplay: string;
  goalMet: boolean;
  dailyGoalMins: number;
  gapDurationMs: number | null;
  gapBand: string | null;
  dbError: string | null;
  tracking: boolean;
  /** ISO datetime when set and in the future; gap prompts suppressed until then. */
  snoozeUntil: string | null;
  /** Note on the running slice when tracking; empty otherwise. */
  currentSliceNote: string;
  /** Present when Copilot usage tracking is enabled. */
  copilotUsage: CopilotUsageSummary | null;
}

export interface Settings {
  idleThresholdMins: number;
  dailyGoalMins: number;
  switchShortcut: string;
  autostart: boolean;
  theme: "dark" | "light";
  trackerAlwaysOnTop: boolean;
  trackerVisible: boolean;
  trackerShowSeconds: boolean;
  jiraEnabled: boolean;
  jiraSiteUrl: string;
  jiraEmail: string;
  jiraApiToken: string;
  /** Optional HTTP CONNECT proxy (`http://host:port`). Empty = none. */
  jiraProxyUrl: string;
  jiraLastError: string | null;
  jiraLastSyncedAt: string | null;
  copilotEnabled: boolean;
  /** github.com or Enterprise Cloud host (e.g. company.ghe.com). */
  copilotGithubHost: string;
  copilotUsername: string;
  copilotApiToken: string;
  /** Optional HTTP CONNECT proxy (`http://host:port`). Empty = none. */
  copilotProxyUrl: string;
  copilotPollIntervalMins: number;
  /** Manual monthly allowance when API entitlement is missing/unlimited. */
  copilotMonthlyLimitOverride: number | null;
  copilotLastError: string | null;
  copilotLastSyncedAt: string | null;
}

export interface JiraConnectionTestEvent {
  phase: "validating" | "connecting" | "verifying" | "succeeded" | "failed" | string;
  message: string;
}

export interface CopilotConnectionTestEvent {
  phase: "validating" | "connecting" | "verifying" | "succeeded" | "failed" | string;
  message: string;
}

export interface TimeSlice {
  id: string;
  taskId: string;
  start: string;
  end: string | null;
  note: string;
}

export interface TimesheetCell {
  taskId: string;
  day: string;
  durationMs: number;
  display: string;
  /** Combined notes for this task-day (` · ` joined). */
  notes: string;
}

export interface TimesheetRow {
  task: Task;
  cells: TimesheetCell[];
  totalMs: number;
  totalDisplay: string;
}

export interface Timesheet {
  weekStart: string;
  days: string[];
  rows: TimesheetRow[];
  dayTotalsMs: number[];
  dayTotalsDisplay: string[];
  grandTotalMs: number;
  grandTotalDisplay: string;
  csv: string;
}

export interface InsightsTaskTotal {
  taskId: string;
  name: string;
  color: string;
  totalMs: number;
}

export interface InsightsWeekTotal {
  weekStart: string;
  totalMs: number;
}

export interface InsightsReport {
  rangeStart: string;
  rangeEnd: string;
  totalMs: number;
  maxDayMs: number;
  avgDayMs: number | null;
  /** Mon=0 … Sun=6 */
  avgByWeekdayMs: (number | null)[];
  earliestStartMins: number | null;
  latestFinishMins: number | null;
  avgStartMins: number | null;
  avgFinishMins: number | null;
  avgLunchMs: number | null;
  qualifyingDays: number;
  /** 0–1 */
  goalHitRate: number | null;
  avgSessionMs: number | null;
  discardedMs: number;
  tasks: InsightsTaskTotal[];
  weekly: InsightsWeekTotal[];
  byHourMs: number[];
}
