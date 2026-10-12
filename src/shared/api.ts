import { invoke } from "@tauri-apps/api/core";
import type {
  Settings,
  Task,
  TaskBookingSummary,
  TimeSlice,
  Timesheet,
  TrackerState,
} from "./types";

export const api = {
  getTrackerState: () => invoke<TrackerState>("get_tracker_state"),
  listTasks: (includeArchived = true) =>
    invoke<Task[]>("list_tasks", { includeArchived }),
  createTask: (input: {
    name: string;
    color?: string;
    description?: string;
    notes?: string;
  }) => invoke<Task>("create_task", { input }),
  updateTask: (input: {
    id: string;
    name?: string;
    color?: string;
    description?: string;
    notes?: string;
    archived?: boolean;
    starred?: boolean;
  }) => invoke<Task>("update_task", { input }),
  getTaskBookingSummary: (taskId: string) =>
    invoke<TaskBookingSummary>("get_task_booking_summary", { taskId }),
  startTask: (taskId: string) => invoke<TrackerState>("start_task", { taskId }),
  stopTracking: () => invoke<TrackerState>("stop_tracking"),
  switchTask: (taskId: string) => invoke<TrackerState>("switch_task", { taskId }),
  createAndStart: (name: string) =>
    invoke<TrackerState>("create_and_start", { name }),
  resolveGapPrevious: () => invoke<TrackerState>("resolve_gap_previous"),
  resolveGapOther: (taskId: string) =>
    invoke<TrackerState>("resolve_gap_other", { taskId }),
  resolveGapDiscard: () => invoke<TrackerState>("resolve_gap_discard"),
  getSettings: () => invoke<Settings>("get_settings"),
  updateSettings: (settings: Settings) =>
    invoke<Settings>("update_settings", { settings }),
  getTimesheet: (weekStart: string) =>
    invoke<Timesheet>("get_timesheet", { weekStart }),
  listSlicesForDay: (day: string) =>
    invoke<TimeSlice[]>("list_slices_for_day", { day }),
  createSlice: (input: { taskId: string; start: string; end: string }) =>
    invoke<TimeSlice>("create_slice", { input }),
  updateSlice: (input: {
    id: string;
    taskId?: string;
    start?: string;
    end?: string;
  }) => invoke<TimeSlice>("update_slice", { input }),
  deleteSlice: (id: string) => invoke<void>("delete_slice", { id }),
  splitSlice: (input: { id: string; at: string }) =>
    invoke<[TimeSlice, TimeSlice]>("split_slice", { input }),
  moveSlice: (id: string, taskId: string) =>
    invoke<TimeSlice>("move_slice", { id, taskId }),
  getDbPath: () => invoke<string>("get_db_path"),
  revealDatabase: () => invoke<void>("reveal_database"),
  exportDatabase: (dest: string) => invoke<void>("export_database", { dest }),
  importDatabase: (source: string) =>
    invoke<void>("import_database", { source }),
  openSwitcher: () => invoke<void>("open_switcher"),
  closeSwitcher: () => invoke<void>("close_switcher"),
  openMainWindow: () => invoke<void>("open_main_window"),
  setTrackerVisible: (visible: boolean) =>
    invoke<void>("set_tracker_visible", { visible }),
  resetTrackerPosition: () => invoke<void>("reset_tracker_position"),
  setTrackerAlwaysOnTop: (alwaysOnTop: boolean) =>
    invoke<Settings>("set_tracker_always_on_top", { alwaysOnTop }),
  currentWeekStart: () => invoke<string>("current_week_start"),
  shiftWeek: (weekStart: string, deltaWeeks: number) =>
    invoke<string>("shift_week", { weekStart, deltaWeeks }),
};
