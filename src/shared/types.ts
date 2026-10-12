export interface Task {
  id: string;
  name: string;
  color: string;
  description: string;
  notes: string;
  archived: boolean;
  starred: boolean;
  createdAt: string;
  updatedAt: string;
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
}

export interface Settings {
  idleThresholdMins: number;
  dailyGoalMins: number;
  switchShortcut: string;
  autostart: boolean;
  theme: "dark" | "light";
  trackerAlwaysOnTop: boolean;
  trackerVisible: boolean;
}

export interface TimeSlice {
  id: string;
  taskId: string;
  start: string;
  end: string | null;
}

export interface TimesheetCell {
  taskId: string;
  day: string;
  durationMs: number;
  display: string;
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
