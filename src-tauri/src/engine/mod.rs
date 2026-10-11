use crate::db::Db;
use crate::domain::*;
use chrono::{Duration, Local, Utc};
use parking_lot::Mutex;
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager, WebviewWindow};

pub struct Engine {
    db: Arc<Db>,
    phase: Mutex<EnginePhase>,
    last_activity: Mutex<chrono::DateTime<Utc>>,
    db_error: Mutex<Option<String>>,
    previous_task_id: Mutex<Option<String>>,
}

impl Engine {
    pub fn new(db: Arc<Db>) -> Arc<Self> {
        let engine = Arc::new(Self {
            db,
            phase: Mutex::new(EnginePhase::Stopped),
            last_activity: Mutex::new(Utc::now()),
            db_error: Mutex::new(None),
            previous_task_id: Mutex::new(None),
        });
        if let Err(e) = engine.reconcile_on_startup() {
            *engine.db_error.lock() = Some(e);
        }
        engine
    }

    pub fn db(&self) -> &Arc<Db> {
        &self.db
    }

    fn set_error(&self, e: String) {
        *self.db_error.lock() = Some(e);
    }

    fn clear_error(&self) {
        *self.db_error.lock() = None;
    }

    pub fn touch_activity(&self) {
        *self.last_activity.lock() = Utc::now();
    }

    pub fn last_activity(&self) -> chrono::DateTime<Utc> {
        *self.last_activity.lock()
    }

    fn reconcile_on_startup(&self) -> Result<(), String> {
        if let Some(slice) = self.db.running_slice()? {
            let settings = self.db.get_settings()?;
            let now = Utc::now();
            let idle_threshold = Duration::minutes(settings.idle_threshold_mins as i64);
            *self.phase.lock() = EnginePhase::Tracking {
                task_id: slice.task_id.clone(),
                slice_id: slice.id.clone(),
            };
            *self.previous_task_id.lock() = Some(slice.task_id.clone());
            // Orphan slice after crash/quit: treat slice start as last activity when stale.
            if now - slice.start >= idle_threshold {
                *self.last_activity.lock() = slice.start;
                self.begin_gap(slice.start)?;
            } else {
                self.touch_activity();
            }
        }
        Ok(())
    }

    pub fn snapshot(&self) -> Result<TrackerState, String> {
        let phase = self.phase.lock().clone();
        let settings = self.db.get_settings()?;
        let now = Utc::now();
        let daily_total_ms = self.db.daily_total_ms(now)?;
        let goal_ms = settings.daily_goal_mins as i64 * 60_000;
        let (current_task, previous_task, gap_duration_ms, gap_band_s, tracking) = match &phase {
            EnginePhase::Stopped => {
                let prev = self
                    .previous_task_id
                    .lock()
                    .clone()
                    .and_then(|id| self.db.get_task(&id).ok().flatten());
                (None, prev, None, None, false)
            }
            EnginePhase::Tracking { task_id, .. } => {
                let t = self.db.get_task(task_id)?;
                (t.clone(), t, None, None, true)
            }
            EnginePhase::NeedsAttribution {
                gap_start,
                previous_task_id,
            } => {
                let prev = self.db.get_task(previous_task_id)?;
                let dur = (now - *gap_start).num_milliseconds().max(0);
                (None, prev, Some(dur), Some(gap_band(dur).to_string()), false)
            }
        };
        Ok(TrackerState {
            phase,
            current_task,
            previous_task,
            daily_total_ms,
            daily_total_display: format_hhmm(daily_total_ms),
            goal_met: daily_total_ms >= goal_ms,
            daily_goal_mins: settings.daily_goal_mins,
            gap_duration_ms,
            gap_band: gap_band_s,
            db_error: self.db_error.lock().clone(),
            tracking,
        })
    }

    pub fn emit_state(&self, app: &AppHandle) {
        match self.snapshot() {
            Ok(state) => {
                let _ = app.emit("tracker-state", &state);
            }
            Err(e) => {
                self.set_error(e.clone());
                let _ = app.emit("db-error", e);
            }
        }
    }

    pub fn start_task(&self, task_id: &str) -> Result<TrackerState, String> {
        self.clear_error();
        if matches!(*self.phase.lock(), EnginePhase::NeedsAttribution { .. }) {
            return Err("Resolve the pending time gap first".into());
        }
        let task = self
            .db
            .get_task(task_id)?
            .ok_or_else(|| "Task not found".to_string())?;
        if task.archived {
            return Err("Cannot track an archived task".into());
        }
        let now = Utc::now();
        if let EnginePhase::Tracking { slice_id, .. } = self.phase.lock().clone() {
            self.db.close_slice(&slice_id, now)?;
        }
        let slice = self.db.open_slice(task_id, now)?;
        *self.phase.lock() = EnginePhase::Tracking {
            task_id: task_id.to_string(),
            slice_id: slice.id,
        };
        *self.previous_task_id.lock() = Some(task_id.to_string());
        self.touch_activity();
        self.snapshot()
    }

    pub fn stop(&self) -> Result<TrackerState, String> {
        self.clear_error();
        if matches!(*self.phase.lock(), EnginePhase::NeedsAttribution { .. }) {
            return Err("Resolve the pending time gap first".into());
        }
        let now = Utc::now();
        if let EnginePhase::Tracking { slice_id, task_id } = self.phase.lock().clone() {
            self.db.close_slice(&slice_id, now)?;
            *self.previous_task_id.lock() = Some(task_id);
        }
        *self.phase.lock() = EnginePhase::Stopped;
        self.touch_activity();
        self.snapshot()
    }

    pub fn switch_task(&self, task_id: &str) -> Result<TrackerState, String> {
        self.start_task(task_id)
    }

    pub fn create_and_start(&self, name: String) -> Result<TrackerState, String> {
        let task = self.db.create_task(CreateTaskInput {
            name,
            color: None,
            description: None,
            notes: None,
        })?;
        self.start_task(&task.id)
    }

    pub fn begin_gap(&self, gap_start: chrono::DateTime<Utc>) -> Result<(), String> {
        let phase = self.phase.lock().clone();
        let (task_id, slice_id) = match phase {
            EnginePhase::Tracking { task_id, slice_id } => (task_id, slice_id),
            EnginePhase::NeedsAttribution { .. } => return Ok(()),
            EnginePhase::Stopped => return Ok(()),
        };
        // Close active work at gap_start (time before idle stays on task)
        let slice = self
            .db
            .get_slice(&slice_id)?
            .ok_or_else(|| "Missing running slice".to_string())?;
        if gap_start > slice.start {
            self.db.close_slice(&slice_id, gap_start)?;
        } else {
            // gap starts at/before slice start — delete zero-length work
            self.db.delete_slice(&slice_id)?;
        }
        *self.phase.lock() = EnginePhase::NeedsAttribution {
            gap_start,
            previous_task_id: task_id,
        };
        Ok(())
    }

    pub fn check_idle(&self, app: &AppHandle) {
        if matches!(*self.phase.lock(), EnginePhase::NeedsAttribution { .. }) {
            // Keep refreshing gap duration for UI
            self.emit_state(app);
            show_gap_window(app);
            return;
        }
        if !matches!(*self.phase.lock(), EnginePhase::Tracking { .. }) {
            return;
        }
        let settings = match self.db.get_settings() {
            Ok(s) => s,
            Err(e) => {
                self.set_error(e);
                self.emit_state(app);
                return;
            }
        };
        let idle_secs = user_idle::UserIdle::get_time()
            .map(|t| t.as_seconds())
            .unwrap_or(0);
        let threshold = settings.idle_threshold_mins as u64 * 60;
        if idle_secs >= threshold {
            let gap_start = Utc::now() - Duration::seconds(idle_secs as i64);
            if let Err(e) = self.begin_gap(gap_start) {
                self.set_error(e);
            }
            self.emit_state(app);
            show_gap_window(app);
        } else {
            // still active enough — update last activity when idle is low
            if idle_secs < 5 {
                self.touch_activity();
            }
            self.emit_state(app);
        }
    }

    pub fn on_resume_or_startup_gap(&self, app: &AppHandle) {
        if matches!(*self.phase.lock(), EnginePhase::NeedsAttribution { .. }) {
            self.emit_state(app);
            show_gap_window(app);
            return;
        }
        if !matches!(*self.phase.lock(), EnginePhase::Tracking { .. }) {
            return;
        }
        let settings = match self.db.get_settings() {
            Ok(s) => s,
            Err(_) => return,
        };
        let last = self.last_activity();
        let now = Utc::now();
        let threshold = Duration::minutes(settings.idle_threshold_mins as i64);
        if now - last >= threshold {
            if let Err(e) = self.begin_gap(last) {
                self.set_error(e);
            }
            self.emit_state(app);
            show_gap_window(app);
        }
    }

    pub fn resolve_gap_previous(&self) -> Result<TrackerState, String> {
        let (gap_start, prev_id) = self.take_gap()?;
        let now = Utc::now();
        if now > gap_start {
            self.db.insert_closed_slice(&prev_id, gap_start, now)?;
        }
        self.start_task(&prev_id)
    }

    pub fn resolve_gap_other(&self, task_id: &str) -> Result<TrackerState, String> {
        let (gap_start, _) = self.take_gap()?;
        let now = Utc::now();
        if now > gap_start {
            self.db.insert_closed_slice(task_id, gap_start, now)?;
        }
        self.start_task(task_id)
    }

    pub fn resolve_gap_discard(&self) -> Result<TrackerState, String> {
        let (gap_start, prev_id) = self.take_gap()?;
        let now = Utc::now();
        if now > gap_start {
            self.db.insert_discard(gap_start, now)?;
        }
        self.start_task(&prev_id)
    }

    fn take_gap(&self) -> Result<(chrono::DateTime<Utc>, String), String> {
        let phase = self.phase.lock().clone();
        match phase {
            EnginePhase::NeedsAttribution {
                gap_start,
                previous_task_id,
            } => {
                *self.phase.lock() = EnginePhase::Stopped;
                Ok((gap_start, previous_task_id))
            }
            _ => Err("No pending gap".into()),
        }
    }

    pub fn midnight_split(&self) -> Result<(), String> {
        let phase = self.phase.lock().clone();
        let EnginePhase::Tracking { task_id, slice_id } = phase else {
            return Ok(());
        };
        let today = Local::now().date_naive();
        let midnight = local_midnight_on(today);
        let slice = self
            .db
            .get_slice(&slice_id)?
            .ok_or_else(|| "Missing slice".to_string())?;
        if slice.start >= midnight {
            return Ok(());
        }
        // Close at midnight, reopen
        self.db.close_slice(&slice_id, midnight)?;
        let new_slice = self.db.open_slice(&task_id, midnight)?;
        *self.phase.lock() = EnginePhase::Tracking {
            task_id,
            slice_id: new_slice.id,
        };
        Ok(())
    }

    pub fn timesheet(&self, week_start: &str) -> Result<Timesheet, String> {
        let start_day = parse_day(week_start)?;
        let week_start_day = week_start_monday(start_day);
        let mut days = Vec::new();
        for i in 0..7 {
            days.push(format_day(week_start_day + Duration::days(i)));
        }
        let (range_start, _) = local_day_bounds(week_start_day);
        let (range_end, _) = local_day_bounds(week_start_day + Duration::days(7));
        let slices = self.db.list_slices_between(range_start, range_end)?;
        let now = Utc::now();

        // Collect task ids with time
        let mut task_ids: Vec<String> = slices.iter().map(|s| s.task_id.clone()).collect();
        task_ids.sort();
        task_ids.dedup();

        let mut rows = Vec::new();
        let mut day_totals = vec![0i64; 7];

        for tid in task_ids {
            let task = self
                .db
                .get_task(&tid)?
                .ok_or_else(|| "Task missing".to_string())?;
            let mut cells = Vec::new();
            let mut row_total = 0i64;
            for (i, day_str) in days.iter().enumerate() {
                let day = parse_day(day_str)?;
                let (d0, d1) = local_day_bounds(day);
                let mut ms = 0i64;
                for s in &slices {
                    if s.task_id != tid {
                        continue;
                    }
                    let end = s.end.unwrap_or(now);
                    ms += overlap_ms(s.start, end, d0, d1);
                }
                row_total += ms;
                day_totals[i] += ms;
                cells.push(TimesheetCell {
                    task_id: tid.clone(),
                    day: day_str.clone(),
                    duration_ms: ms,
                    display: format_hhmm(ms),
                });
            }
            rows.push(TimesheetRow {
                task,
                cells,
                total_ms: row_total,
                total_display: format_hhmm(row_total),
            });
        }

        rows.sort_by(|a, b| {
            a.task
                .name
                .to_lowercase()
                .cmp(&b.task.name.to_lowercase())
        });

        let grand = day_totals.iter().sum();
        let day_totals_display: Vec<String> = day_totals.iter().map(|m| format_hhmm(*m)).collect();
        let csv = build_csv(&days, &rows, &day_totals_display, &format_hhmm(grand));

        Ok(Timesheet {
            week_start: format_day(week_start_day),
            days,
            rows,
            day_totals_ms: day_totals,
            day_totals_display,
            grand_total_ms: grand,
            grand_total_display: format_hhmm(grand),
            csv,
        })
    }

    pub fn refresh_after_edit(&self) -> Result<TrackerState, String> {
        // If DB no longer has our running slice, stop.
        let phase = self.phase.lock().clone();
        if let EnginePhase::Tracking { slice_id, .. } = phase {
            if self.db.get_slice(&slice_id)?.is_none()
                || self.db.get_slice(&slice_id)?.and_then(|s| s.end).is_some()
            {
                if let Some(running) = self.db.running_slice()? {
                    *self.phase.lock() = EnginePhase::Tracking {
                        task_id: running.task_id.clone(),
                        slice_id: running.id,
                    };
                    *self.previous_task_id.lock() = Some(running.task_id);
                } else {
                    *self.phase.lock() = EnginePhase::Stopped;
                }
            }
        }
        self.snapshot()
    }
}

fn build_csv(
    days: &[String],
    rows: &[TimesheetRow],
    day_totals: &[String],
    grand: &str,
) -> String {
    let mut out = String::from("Task");
    for d in days {
        out.push(',');
        out.push_str(d);
    }
    out.push_str(",Total\n");
    for row in rows {
        out.push('"');
        out.push_str(&row.task.name.replace('"', "\"\""));
        out.push('"');
        for c in &row.cells {
            out.push(',');
            out.push_str(&c.display);
        }
        out.push(',');
        out.push_str(&row.total_display);
        out.push('\n');
    }
    out.push_str("Total");
    for t in day_totals {
        out.push(',');
        out.push_str(t);
    }
    out.push(',');
    out.push_str(grand);
    out.push('\n');
    out
}

pub fn show_gap_window(app: &AppHandle) {
    let Some(engine) = app.try_state::<Arc<Engine>>() else {
        return;
    };
    if !matches!(
        *engine.phase.lock(),
        EnginePhase::NeedsAttribution { .. }
    ) {
        hide_gap_window(app);
        return;
    }
    if let Some(w) = app.get_webview_window("gap") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_gap_window(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("gap") {
        let _ = w.hide();
    }
}

pub fn show_switcher(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("switcher") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_switcher(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("switcher") {
        let _ = w.hide();
    }
}

pub fn show_main(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn set_tracker_visible(app: &AppHandle, visible: bool) {
    if let Some(w) = app.get_webview_window("tracker") {
        if visible {
            let _ = w.show();
        } else {
            let _ = w.hide();
        }
    }
}

pub fn position_tracker(window: &WebviewWindow) {
    // Place near top-right on Mac; Windows dock handled in chrome module.
    if let Ok(Some(monitor)) = window.current_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let width = 280.0;
        let height = 40.0;
        let margin = 12.0;
        #[cfg(target_os = "windows")]
        let y = (size.height as f64 / scale) - height - 48.0 - margin; // above taskbar
        #[cfg(not(target_os = "windows"))]
        let y = margin;
        let x = (size.width as f64 / scale) - width - margin;
        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize { width, height }));
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }
}
