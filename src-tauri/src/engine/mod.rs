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
            // Prefer last checkpoint so quit/crash only gaps post-checkpoint time.
            let mut cutoff = self
                .db
                .get_last_good_at()?
                .unwrap_or(slice.start);
            if cutoff < slice.start {
                cutoff = slice.start;
            }
            if cutoff > now {
                cutoff = now;
            }
            if now - cutoff >= idle_threshold {
                match self.db.get_snooze_until()? {
                    // Still snoozing — keep tracking and refresh the checkpoint.
                    Some(until) if until > now => {
                        self.touch_activity();
                        self.db.set_last_good_at(Some(now))?;
                    }
                    // Snooze expired during downtime — that period was already attributed.
                    Some(_) => {
                        self.finish_snooze()?;
                    }
                    None => {
                        *self.last_activity.lock() = cutoff;
                        self.begin_gap(cutoff)?;
                    }
                }
            } else {
                self.touch_activity();
                self.db.set_last_good_at(Some(now))?;
            }
        }
        Ok(())
    }

    /// Active snooze deadline, clearing the persisted value if it has expired.
    fn active_snooze_until(&self) -> Result<Option<chrono::DateTime<Utc>>, String> {
        match self.db.get_snooze_until()? {
            Some(until) if until > Utc::now() => Ok(Some(until)),
            Some(_) => {
                self.finish_snooze()?;
                Ok(None)
            }
            None => Ok(None),
        }
    }

    pub fn is_snoozed(&self) -> bool {
        matches!(self.active_snooze_until(), Ok(Some(_)))
    }

    /// Clear snooze and reset activity/checkpoint so normal idle detection resumes from now.
    fn finish_snooze(&self) -> Result<(), String> {
        self.db.set_snooze_until(None)?;
        let now = Utc::now();
        self.touch_activity();
        self.db.set_last_good_at(Some(now))?;
        Ok(())
    }

    pub fn start_snooze(&self, until: chrono::DateTime<Utc>) -> Result<TrackerState, String> {
        self.clear_error();
        if matches!(*self.phase.lock(), EnginePhase::NeedsAttribution { .. }) {
            return Err("Resolve the pending time gap first".into());
        }
        if !matches!(*self.phase.lock(), EnginePhase::Tracking { .. }) {
            return Err("Start tracking before snoozing".into());
        }
        if until <= Utc::now() {
            return Err("Snooze end must be in the future".into());
        }
        self.db.set_snooze_until(Some(until))?;
        self.snapshot()
    }

    pub fn clear_snooze(&self) -> Result<TrackerState, String> {
        self.clear_error();
        self.finish_snooze()?;
        self.snapshot()
    }

    pub fn snapshot(&self) -> Result<TrackerState, String> {
        let phase = self.phase.lock().clone();
        let settings = self.db.get_settings()?;
        let now = Utc::now();
        let daily_total_ms = self.db.daily_total_ms(now)?;
        let goal_ms = settings.daily_goal_mins as i64 * 60_000;
        let snooze_until = self.active_snooze_until()?;
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
            snooze_until,
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
        let old_id = match self.phase.lock().clone() {
            EnginePhase::Tracking { slice_id, .. } => Some(slice_id),
            _ => None,
        };
        let slice = self
            .db
            .replace_running_slice(old_id.as_deref(), task_id, now)?;
        *self.phase.lock() = EnginePhase::Tracking {
            task_id: task_id.to_string(),
            slice_id: slice.id,
        };
        *self.previous_task_id.lock() = Some(task_id.to_string());
        self.touch_activity();
        self.db.set_last_good_at(Some(now))?;
        self.snapshot()
    }

    /// Force a stopped, consistent engine after replacing the on-disk database.
    pub fn reset_after_import(&self) -> Result<(), String> {
        self.clear_error();
        if let Some(running) = self.db.running_slice()? {
            let _ = self.db.close_slice(&running.id, Utc::now());
        }
        *self.phase.lock() = EnginePhase::Stopped;
        *self.previous_task_id.lock() = None;
        let _ = self.db.set_last_good_at(None);
        let _ = self.db.set_snooze_until(None);
        self.touch_activity();
        Ok(())
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
        self.db.set_last_good_at(None)?;
        self.db.set_snooze_until(None)?;
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
        if self.is_snoozed() {
            return Ok(());
        }
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
        self.db.set_last_good_at(None)?;
        Ok(())
    }

    /// Persist a tracking heartbeat at most once per minute while the user is active.
    /// Also opens a gap from `last_good_at` when wall-clock jumps past the idle threshold
    /// (e.g. sleep) without a trustworthy OS idle sample.
    pub fn checkpoint_if_due(&self) {
        let idle_secs = user_idle::UserIdle::get_time()
            .map(|t| t.as_seconds())
            .unwrap_or(0);
        if let Err(e) = self.checkpoint_with_idle(idle_secs) {
            self.set_error(e);
        }
    }

    /// Testable core of [`Self::checkpoint_if_due`].
    pub fn checkpoint_with_idle(&self, idle_secs: u64) -> Result<(), String> {
        if !matches!(*self.phase.lock(), EnginePhase::Tracking { .. }) {
            return Ok(());
        }
        let snoozed = self.is_snoozed();
        let settings = self.db.get_settings()?;
        let threshold_secs = settings.idle_threshold_mins as u64 * 60;
        let threshold = Duration::minutes(settings.idle_threshold_mins as i64);
        let now = Utc::now();
        let last_good = self.db.get_last_good_at()?;

        // Live idle path owns the gap once past threshold (unless snoozed).
        if !snoozed && idle_secs >= threshold_secs {
            return Ok(());
        }

        // Sleep / wall-clock jump: require more than one checkpoint period of slack
        // beyond the idle threshold so an active 1-minute setting cannot false-trigger
        // between heartbeats (checkpoint every 60s, poll ~5s).
        const CHECKPOINT_SECS: i64 = 60;
        const POLL_SLACK_SECS: i64 = 15;
        let sleep_cutoff = threshold + Duration::seconds(CHECKPOINT_SECS + POLL_SLACK_SECS);
        if !snoozed {
            if let Some(lg) = last_good {
                if now - lg >= sleep_cutoff {
                    *self.last_activity.lock() = lg;
                    self.begin_gap(lg)?;
                    return Ok(());
                }
            }
        }

        let due = match last_good {
            None => true,
            Some(lg) => now - lg >= Duration::seconds(CHECKPOINT_SECS),
        };
        if due {
            self.db.set_last_good_at(Some(now))?;
        }
        if snoozed || idle_secs < 5 {
            self.touch_activity();
        }
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
        if self.is_snoozed() {
            self.emit_state(app);
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
        if self.is_snoozed() {
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
        let (gap_start, prev_id) = self.peek_gap()?;
        let now = Utc::now();
        if now > gap_start {
            self.db.insert_closed_slice(&prev_id, gap_start, now)?;
        }
        self.clear_gap_phase();
        self.start_task(&prev_id)
    }

    pub fn resolve_gap_other(&self, task_id: &str) -> Result<TrackerState, String> {
        let (gap_start, _) = self.peek_gap()?;
        let now = Utc::now();
        if now > gap_start {
            self.db.insert_closed_slice(task_id, gap_start, now)?;
        }
        self.clear_gap_phase();
        self.start_task(task_id)
    }

    pub fn resolve_gap_discard(&self) -> Result<TrackerState, String> {
        let (gap_start, prev_id) = self.peek_gap()?;
        let now = Utc::now();
        if now > gap_start {
            self.db.insert_discard(gap_start, now)?;
        }
        self.clear_gap_phase();
        self.start_task(&prev_id)
    }

    fn peek_gap(&self) -> Result<(chrono::DateTime<Utc>, String), String> {
        match self.phase.lock().clone() {
            EnginePhase::NeedsAttribution {
                gap_start,
                previous_task_id,
            } => Ok((gap_start, previous_task_id)),
            _ => Err("No pending gap".into()),
        }
    }

    fn clear_gap_phase(&self) {
        if matches!(*self.phase.lock(), EnginePhase::NeedsAttribution { .. }) {
            *self.phase.lock() = EnginePhase::Stopped;
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
        // Close at midnight, reopen atomically
        let new_slice = self
            .db
            .replace_running_slice(Some(&slice_id), &task_id, midnight)?;
        *self.phase.lock() = EnginePhase::Tracking {
            task_id,
            slice_id: new_slice.id,
        };
        // Keep heartbeat aligned with continued tracking after the split.
        let _ = self.db.set_last_good_at(Some(Utc::now()));
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

pub fn show_snooze_picker(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("snooze") {
        let _ = w.center();
        let _ = w.show();
        let _ = w.set_focus();
    }
}

pub fn hide_snooze_picker(app: &AppHandle) {
    if let Some(w) = app.get_webview_window("snooze") {
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

/// Show/hide the tracker and persist `tracker_visible` in settings.
pub fn set_tracker_visible_persisted(app: &AppHandle, engine: &Engine, visible: bool) {
    if let Ok(mut settings) = engine.db().get_settings() {
        settings.tracker_visible = visible;
        let _ = engine.db().update_settings(&settings);
    }
    set_tracker_visible(app, visible);
}

pub fn apply_tracker_always_on_top(app: &AppHandle, always_on_top: bool) {
    if let Some(w) = app.get_webview_window("tracker") {
        let _ = w.set_always_on_top(always_on_top);
    }
}

const TRACKER_WIDTH: f64 = 280.0;
const TRACKER_HEIGHT: f64 = 40.0;
/// Approximate logical width of the system tray + clock on Windows.
#[cfg(target_os = "windows")]
const WINDOWS_TRAY_RESERVE: f64 = 200.0;

pub fn position_tracker(window: &WebviewWindow) {
    if let Ok(Some(monitor)) = window.current_monitor() {
        let size = monitor.size();
        let scale = monitor.scale_factor();
        let mon_w = size.width as f64 / scale;
        let mon_x = monitor.position().x as f64 / scale;
        let mon_y = monitor.position().y as f64 / scale;

        #[cfg(target_os = "windows")]
        let (x, y) = {
            let mon_h = size.height as f64 / scale;
            windows_tracker_dock(mon_x, mon_y, mon_w, mon_h, monitor.work_area(), scale)
        };

        #[cfg(not(target_os = "windows"))]
        let (x, y) = {
            let margin = 12.0;
            (
                mon_x + mon_w - TRACKER_WIDTH - margin,
                mon_y + margin,
            )
        };

        let _ = window.set_size(tauri::Size::Logical(tauri::LogicalSize {
            width: TRACKER_WIDTH,
            height: TRACKER_HEIGHT,
        }));
        let _ = window.set_position(tauri::Position::Logical(tauri::LogicalPosition { x, y }));
    }
}

/// Dock into the taskbar band, left of the system tray (typical bottom taskbar).
#[cfg(target_os = "windows")]
fn windows_tracker_dock(
    mon_x: f64,
    mon_y: f64,
    mon_w: f64,
    mon_h: f64,
    work: &tauri::PhysicalRect<i32, u32>,
    scale: f64,
) -> (f64, f64) {
    let work_x = work.position.x as f64 / scale;
    let work_y = work.position.y as f64 / scale;
    let work_w = work.size.width as f64 / scale;
    let work_h = work.size.height as f64 / scale;
    let work_bottom = work_y + work_h;
    let work_right = work_x + work_w;
    let mon_bottom = mon_y + mon_h;
    let mon_right = mon_x + mon_w;

    if work_bottom < mon_bottom - 1.0 {
        // Bottom taskbar: center in band, left of tray.
        let band_h = mon_bottom - work_bottom;
        let y = if band_h >= TRACKER_HEIGHT {
            work_bottom + (band_h - TRACKER_HEIGHT) / 2.0
        } else {
            mon_bottom - TRACKER_HEIGHT
        };
        let x = mon_right - TRACKER_WIDTH - WINDOWS_TRAY_RESERVE;
        (x, y)
    } else if work_y > mon_y + 1.0 {
        // Top taskbar
        let band_h = work_y - mon_y;
        let y = if band_h >= TRACKER_HEIGHT {
            mon_y + (band_h - TRACKER_HEIGHT) / 2.0
        } else {
            mon_y
        };
        let x = mon_right - TRACKER_WIDTH - WINDOWS_TRAY_RESERVE;
        (x, y)
    } else if work_x > mon_x + 1.0 {
        // Left taskbar
        (work_x, work_bottom - TRACKER_HEIGHT - 4.0)
    } else if work_right < mon_right - 1.0 {
        // Right taskbar
        (work_right - TRACKER_WIDTH, work_bottom - TRACKER_HEIGHT - 4.0)
    } else {
        (
            mon_right - TRACKER_WIDTH - WINDOWS_TRAY_RESERVE,
            mon_bottom - TRACKER_HEIGHT,
        )
    }
}

/// Apply default dock position once (first launch), or force when `force` is true (Reset).
pub fn ensure_tracker_default_position(app: &AppHandle, engine: &Engine, force: bool) {
    let already = engine
        .db()
        .tracker_default_position_applied()
        .unwrap_or(false);
    if already && !force {
        return;
    }
    if let Some(w) = app.get_webview_window("tracker") {
        position_tracker(&w);
        let _ = engine.db().set_tracker_default_position_applied(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Db;
    use crate::test_support::test_db;
    use chrono::{Duration, Local};

    fn make_task(db: &Db, name: &str) -> Task {
        db.create_task(CreateTaskInput {
            name: name.into(),
            color: Some("#17BEBB".into()),
            description: None,
            notes: None,
        })
        .unwrap()
    }

    fn engine_empty() -> (tempfile::TempDir, Arc<Engine>) {
        let (dir, db) = test_db();
        (dir, Engine::new(db))
    }

    #[test]
    fn start_stop_resume() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Focus");
        let state = eng.start_task(&task.id).unwrap();
        assert!(state.tracking);
        assert!(matches!(state.phase, EnginePhase::Tracking { .. }));

        let stopped = eng.stop().unwrap();
        assert!(!stopped.tracking);
        assert_eq!(stopped.previous_task.as_ref().unwrap().id, task.id);

        let resumed = eng.start_task(&task.id).unwrap();
        assert!(resumed.tracking);
    }

    #[test]
    fn switch_tasks_closes_previous() {
        let (_dir, eng) = engine_empty();
        let a = make_task(eng.db(), "A");
        let b = make_task(eng.db(), "B");
        eng.start_task(&a.id).unwrap();
        let first_id = match eng.phase.lock().clone() {
            EnginePhase::Tracking { slice_id, .. } => slice_id,
            _ => panic!("expected tracking"),
        };
        eng.switch_task(&b.id).unwrap();
        let first = eng.db().get_slice(&first_id).unwrap().unwrap();
        assert!(first.end.is_some());
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { task_id, .. } if task_id == b.id
        ));
    }

    #[test]
    fn gap_resolve_previous() {
        let (_dir, eng) = engine_empty();
        let prev = make_task(eng.db(), "Prev");
        eng.start_task(&prev.id).unwrap();
        let gap_start = Utc::now() - Duration::minutes(30);
        eng.begin_gap(gap_start).unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::NeedsAttribution { .. }
        ));
        eng.resolve_gap_previous().unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { task_id, .. } if task_id == prev.id
        ));
    }

    #[test]
    fn gap_resolve_other() {
        let (_dir, eng) = engine_empty();
        let prev = make_task(eng.db(), "Prev");
        let other = make_task(eng.db(), "Other");
        eng.start_task(&prev.id).unwrap();
        eng.begin_gap(Utc::now() - Duration::minutes(15)).unwrap();
        eng.resolve_gap_other(&other.id).unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { task_id, .. } if task_id == other.id
        ));
    }

    #[test]
    fn gap_resolve_discard() {
        let (_dir, eng) = engine_empty();
        let prev = make_task(eng.db(), "Prev");
        eng.start_task(&prev.id).unwrap();
        eng.begin_gap(Utc::now() - Duration::minutes(10)).unwrap();
        eng.resolve_gap_discard().unwrap();
        let discards: i64 = eng
            .db()
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM discard_log", [], |r| r.get(0))
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        assert_eq!(discards, 1);
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { task_id, .. } if task_id == prev.id
        ));
    }

    #[test]
    fn start_stop_blocked_during_gap() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Blocked");
        eng.start_task(&task.id).unwrap();
        eng.begin_gap(Utc::now() - Duration::minutes(20)).unwrap();
        assert!(eng.start_task(&task.id).unwrap_err().contains("gap"));
        assert!(eng.stop().unwrap_err().contains("gap"));
    }

    #[test]
    fn failed_gap_attribution_keeps_pending() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Overlap");
        eng.start_task(&task.id).unwrap();
        let gap_start = Utc::now() - Duration::minutes(40);
        eng.begin_gap(gap_start).unwrap();

        // Plant an overlapping closed slice so attribution insert fails.
        let mid = gap_start + Duration::minutes(5);
        eng.db()
            .insert_closed_slice(&task.id, mid, mid + Duration::minutes(10))
            .unwrap();

        let err = eng.resolve_gap_previous().unwrap_err();
        assert!(err.contains("overlaps") || err.contains("overlap"));
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::NeedsAttribution { .. }
        ));
    }

    #[test]
    fn startup_reconcile_stale_and_fresh() {
        // Stale running slice → NeedsAttribution
        {
            let (_dir, db) = test_db();
            let task = make_task(&db, "Stale");
            let start = Utc::now() - Duration::hours(2);
            db.replace_running_slice(None, &task.id, start).unwrap();
            let eng = Engine::new(db);
            assert!(matches!(
                eng.phase.lock().clone(),
                EnginePhase::NeedsAttribution { .. }
            ));
        }
        // Fresh running slice → Tracking
        {
            let (_dir, db) = test_db();
            let task = make_task(&db, "Fresh");
            let start = Utc::now() - Duration::minutes(1);
            db.replace_running_slice(None, &task.id, start).unwrap();
            let eng = Engine::new(db);
            assert!(matches!(
                eng.phase.lock().clone(),
                EnginePhase::Tracking { .. }
            ));
        }
    }

    #[test]
    fn checkpoint_advances_last_good_at() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Checkpoint");
        eng.start_task(&task.id).unwrap();
        let first = eng.db().get_last_good_at().unwrap().unwrap();
        // Simulate an older checkpoint so the next write is due.
        let older = first - Duration::seconds(61);
        eng.db().set_last_good_at(Some(older)).unwrap();
        eng.checkpoint_with_idle(0).unwrap();
        let next = eng.db().get_last_good_at().unwrap().unwrap();
        assert!(next > older);
        assert!(next >= first);
    }

    #[test]
    fn checkpoint_one_minute_threshold_does_not_false_gap() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "OneMin");
        eng.start_task(&task.id).unwrap();
        let mut settings = eng.db().get_settings().unwrap();
        settings.idle_threshold_mins = 1;
        eng.db().update_settings(&settings).unwrap();

        let first = eng.db().get_last_good_at().unwrap().unwrap();
        let older = first - Duration::seconds(61);
        eng.db().set_last_good_at(Some(older)).unwrap();
        eng.checkpoint_with_idle(0).unwrap();

        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));
        let next = eng.db().get_last_good_at().unwrap().unwrap();
        assert!(next > older);
    }

    #[test]
    fn checkpoint_sleep_cutoff_still_opens_gap() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Slept");
        eng.start_task(&task.id).unwrap();
        let mut settings = eng.db().get_settings().unwrap();
        settings.idle_threshold_mins = 1;
        eng.db().update_settings(&settings).unwrap();

        let first = eng.db().get_last_good_at().unwrap().unwrap();
        // threshold (60s) + checkpoint (60s) + slack (15s) = 135s
        let older = first - Duration::seconds(140);
        eng.db().set_last_good_at(Some(older)).unwrap();
        eng.checkpoint_with_idle(0).unwrap();

        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::NeedsAttribution { .. }
        ));
    }

    #[test]
    fn startup_reconcile_uses_checkpoint_not_slice_start() {
        let (_dir, db) = test_db();
        let task = make_task(&db, "Quit");
        let start = Utc::now() - Duration::hours(2);
        let checkpoint = Utc::now() - Duration::minutes(30);
        let slice = db.replace_running_slice(None, &task.id, start).unwrap();
        db.set_last_good_at(Some(checkpoint)).unwrap();

        let eng = Engine::new(db);
        match eng.phase.lock().clone() {
            EnginePhase::NeedsAttribution { gap_start, .. } => {
                let delta = (gap_start - checkpoint).num_seconds().abs();
                assert!(delta <= 1, "gap_start={gap_start} checkpoint={checkpoint}");
            }
            other => panic!("expected NeedsAttribution, got {other:?}"),
        }
        let closed = eng.db().get_slice(&slice.id).unwrap().unwrap();
        assert!(closed.end.is_some());
        let end = closed.end.unwrap();
        let delta = (end - checkpoint).num_seconds().abs();
        assert!(delta <= 1, "end={end} checkpoint={checkpoint}");
        assert_ne!(closed.end, Some(start));
    }

    #[test]
    fn startup_reconcile_recent_checkpoint_resumes() {
        let (_dir, db) = test_db();
        let task = make_task(&db, "Resume");
        let start = Utc::now() - Duration::hours(2);
        let checkpoint = Utc::now() - Duration::minutes(2);
        db.replace_running_slice(None, &task.id, start).unwrap();
        db.set_last_good_at(Some(checkpoint)).unwrap();

        let eng = Engine::new(db);
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));
        assert!(eng.db().running_slice().unwrap().is_some());
        assert!(eng.db().get_last_good_at().unwrap().is_some());
    }

    #[test]
    fn midnight_split_closes_and_reopens() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Midnight");
        let today = Local::now().date_naive();
        let midnight = local_midnight_on(today);
        let before = midnight - Duration::hours(2);
        let slice = eng
            .db()
            .replace_running_slice(None, &task.id, before)
            .unwrap();
        *eng.phase.lock() = EnginePhase::Tracking {
            task_id: task.id.clone(),
            slice_id: slice.id.clone(),
        };
        eng.midnight_split().unwrap();
        let old = eng.db().get_slice(&slice.id).unwrap().unwrap();
        assert_eq!(old.end, Some(midnight));
        let phase = eng.phase.lock().clone();
        match phase {
            EnginePhase::Tracking { slice_id, .. } => {
                assert_ne!(slice_id, slice.id);
                let neu = eng.db().get_slice(&slice_id).unwrap().unwrap();
                assert_eq!(neu.start, midnight);
                assert!(neu.end.is_none());
            }
            other => panic!("unexpected phase {other:?}"),
        }
    }

    #[test]
    fn timesheet_totals_and_csv_escape() {
        let (_dir, eng) = engine_empty();
        let task = eng
            .db()
            .create_task(CreateTaskInput {
                name: r#"Say "hi""#.into(),
                color: Some("#E4572E".into()),
                description: None,
                notes: None,
            })
            .unwrap();
        let monday = week_start_monday(Local::now().date_naive());
        let (d0, _) = local_day_bounds(monday);
        eng.db()
            .insert_closed_slice(&task.id, d0 + Duration::hours(9), d0 + Duration::hours(11))
            .unwrap();
        let sheet = eng.timesheet(&format_day(monday)).unwrap();
        assert_eq!(sheet.week_start, format_day(monday));
        assert_eq!(sheet.rows.len(), 1);
        assert_eq!(sheet.rows[0].total_ms, 2 * 3_600_000);
        assert!(sheet.csv.contains("Say \"\"hi\"\""));
        assert_eq!(sheet.grand_total_ms, 2 * 3_600_000);
    }

    #[test]
    fn reset_after_import_stops() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Import");
        eng.start_task(&task.id).unwrap();
        eng.reset_after_import().unwrap();
        assert!(matches!(eng.phase.lock().clone(), EnginePhase::Stopped));
        assert!(eng.db().running_slice().unwrap().is_none());
    }

    #[test]
    fn archived_and_missing_task_errors() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "ArchiveMe");
        eng.db()
            .update_task(UpdateTaskInput {
                id: task.id.clone(),
                name: None,
                color: None,
                description: None,
                notes: None,
                archived: Some(true),
                starred: None,
            })
            .unwrap();
        assert!(eng.start_task(&task.id).unwrap_err().contains("archived"));
        assert!(eng.start_task("missing-id").unwrap_err().contains("not found"));
    }

    #[test]
    fn refresh_after_edit_stops_when_slice_gone() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Edit");
        eng.start_task(&task.id).unwrap();
        let slice_id = match eng.phase.lock().clone() {
            EnginePhase::Tracking { slice_id, .. } => slice_id,
            _ => panic!("tracking"),
        };
        eng.db().delete_slice(&slice_id).unwrap();
        let state = eng.refresh_after_edit().unwrap();
        assert!(!state.tracking);
        assert!(matches!(state.phase, EnginePhase::Stopped));
    }

    #[test]
    fn snooze_rejects_when_stopped() {
        let (_dir, eng) = engine_empty();
        let until = Utc::now() + Duration::hours(1);
        assert!(eng.start_snooze(until).unwrap_err().contains("tracking"));
    }

    #[test]
    fn snooze_rejects_during_gap() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "GapSnooze");
        eng.start_task(&task.id).unwrap();
        eng.begin_gap(Utc::now() - Duration::minutes(10)).unwrap();
        let until = Utc::now() + Duration::hours(1);
        assert!(eng.start_snooze(until).unwrap_err().contains("gap"));
    }

    #[test]
    fn snooze_suppresses_begin_gap_and_sleep_checkpoint() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "SnoozeKeep");
        eng.start_task(&task.id).unwrap();
        let until = Utc::now() + Duration::hours(2);
        eng.start_snooze(until).unwrap();
        assert!(eng.is_snoozed());

        eng.begin_gap(Utc::now() - Duration::minutes(30)).unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));

        let first = eng.db().get_last_good_at().unwrap().unwrap();
        let older = first - Duration::minutes(20);
        eng.db().set_last_good_at(Some(older)).unwrap();
        eng.checkpoint_with_idle(0).unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));
        let next = eng.db().get_last_good_at().unwrap().unwrap();
        assert!(next > older);

        // High idle still checkpoints while snoozed.
        eng.db().set_last_good_at(Some(Utc::now() - Duration::seconds(61))).unwrap();
        eng.checkpoint_with_idle(600).unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));
        assert!(eng.db().get_last_good_at().unwrap().is_some());
    }

    #[test]
    fn clear_snooze_allows_gaps_again() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "ExitSnooze");
        eng.start_task(&task.id).unwrap();
        eng.start_snooze(Utc::now() + Duration::hours(1)).unwrap();
        eng.clear_snooze().unwrap();
        assert!(!eng.is_snoozed());
        eng.begin_gap(Utc::now() - Duration::minutes(15)).unwrap();
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::NeedsAttribution { .. }
        ));
    }

    #[test]
    fn stop_clears_snooze() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "StopSnooze");
        eng.start_task(&task.id).unwrap();
        eng.start_snooze(Utc::now() + Duration::hours(1)).unwrap();
        eng.stop().unwrap();
        assert!(eng.db().get_snooze_until().unwrap().is_none());
        assert!(eng.snapshot().unwrap().snooze_until.is_none());
    }

    #[test]
    fn expired_snooze_clears_on_snapshot() {
        let (_dir, eng) = engine_empty();
        let task = make_task(eng.db(), "Expire");
        eng.start_task(&task.id).unwrap();
        eng.db()
            .set_snooze_until(Some(Utc::now() - Duration::minutes(1)))
            .unwrap();
        let state = eng.snapshot().unwrap();
        assert!(state.snooze_until.is_none());
        assert!(eng.db().get_snooze_until().unwrap().is_none());
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));
    }

    #[test]
    fn startup_reconcile_skips_gap_while_snoozed() {
        let (_dir, db) = test_db();
        let task = make_task(&db, "SnoozeStartup");
        let start = Utc::now() - Duration::hours(2);
        db.replace_running_slice(None, &task.id, start).unwrap();
        db.set_last_good_at(Some(Utc::now() - Duration::hours(1)))
            .unwrap();
        db.set_snooze_until(Some(Utc::now() + Duration::hours(1)))
            .unwrap();
        let eng = Engine::new(db);
        assert!(matches!(
            eng.phase.lock().clone(),
            EnginePhase::Tracking { .. }
        ));
        assert!(eng.is_snoozed());
    }
}
