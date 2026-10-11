use crate::domain::*;
use crate::engine::{hide_gap_window, hide_switcher, show_main, show_switcher, Engine};
use chrono::Duration;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_autostart::ManagerExt;

#[tauri::command]
pub fn get_tracker_state(engine: State<'_, Arc<Engine>>) -> Result<TrackerState, String> {
    engine.snapshot()
}

#[tauri::command]
pub fn list_tasks(
    engine: State<'_, Arc<Engine>>,
    include_archived: bool,
) -> Result<Vec<Task>, String> {
    engine.db().list_tasks(include_archived)
}

#[tauri::command]
pub fn create_task(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    input: CreateTaskInput,
) -> Result<Task, String> {
    let task = engine.db().create_task(input)?;
    engine.emit_state(&app);
    Ok(task)
}

#[tauri::command]
pub fn update_task(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    input: UpdateTaskInput,
) -> Result<Task, String> {
    let task = engine.db().update_task(input)?;
    engine.emit_state(&app);
    Ok(task)
}

#[tauri::command]
pub fn start_task(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    task_id: String,
) -> Result<TrackerState, String> {
    let state = engine.start_task(&task_id)?;
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn stop_tracking(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<TrackerState, String> {
    let state = engine.stop()?;
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn switch_task(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    task_id: String,
) -> Result<TrackerState, String> {
    let state = engine.switch_task(&task_id)?;
    hide_switcher(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn create_and_start(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    name: String,
) -> Result<TrackerState, String> {
    let state = engine.create_and_start(name)?;
    hide_switcher(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn resolve_gap_previous(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<TrackerState, String> {
    let state = engine.resolve_gap_previous()?;
    hide_gap_window(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn resolve_gap_other(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    task_id: String,
) -> Result<TrackerState, String> {
    let state = engine.resolve_gap_other(&task_id)?;
    hide_gap_window(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn resolve_gap_discard(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<TrackerState, String> {
    let state = engine.resolve_gap_discard()?;
    hide_gap_window(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn get_settings(engine: State<'_, Arc<Engine>>) -> Result<Settings, String> {
    engine.db().get_settings()
}

#[tauri::command]
pub fn update_settings(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    settings: Settings,
) -> Result<Settings, String> {
    if settings.idle_threshold_mins == 0 {
        return Err("Idle threshold must be at least 1 minute".into());
    }
    if settings.daily_goal_mins == 0 {
        return Err("Daily goal must be at least 1 minute".into());
    }
    engine.db().update_settings(&settings)?;
    if settings.autostart {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }
    engine.emit_state(&app);
    let _ = app.emit("settings-changed", &settings);
    Ok(settings)
}

#[tauri::command]
pub fn get_timesheet(
    engine: State<'_, Arc<Engine>>,
    week_start: String,
) -> Result<Timesheet, String> {
    engine.timesheet(&week_start)
}

#[tauri::command]
pub fn list_slices_for_day(
    engine: State<'_, Arc<Engine>>,
    day: String,
) -> Result<Vec<TimeSlice>, String> {
    let d = parse_day(&day)?;
    let (start, end) = local_day_bounds(d);
    engine.db().coalesce_near_adjacent()?;
    engine.db().list_slices_for_day_display(start, end)
}

#[tauri::command]
pub fn create_slice(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    input: CreateSliceInput,
) -> Result<TimeSlice, String> {
    let slice = engine
        .db()
        .insert_closed_slice(&input.task_id, input.start, input.end)?;
    let merged = engine.db().merge_adjacent_same_task(&slice.id)?;
    engine.db().coalesce_near_adjacent()?;
    let out = engine
        .db()
        .get_slice(&merged.id)?
        .unwrap_or(merged);
    let _ = engine.refresh_after_edit()?;
    engine.emit_state(&app);
    Ok(out)
}

#[tauri::command]
pub fn update_slice(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    input: UpdateSliceInput,
) -> Result<TimeSlice, String> {
    let slice = engine.db().update_slice(input)?;
    let merged = engine.db().merge_adjacent_same_task(&slice.id)?;
    engine.db().coalesce_near_adjacent()?;
    let out = engine
        .db()
        .get_slice(&merged.id)?
        .unwrap_or(merged);
    let _ = engine.refresh_after_edit()?;
    engine.emit_state(&app);
    Ok(out)
}

#[tauri::command]
pub fn delete_slice(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    id: String,
) -> Result<(), String> {
    engine.db().delete_slice(&id)?;
    let _ = engine.refresh_after_edit()?;
    engine.emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn split_slice(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    input: SplitSliceInput,
) -> Result<(TimeSlice, TimeSlice), String> {
    let result = engine.db().split_slice(input)?;
    let _ = engine.refresh_after_edit()?;
    engine.emit_state(&app);
    Ok(result)
}

#[tauri::command]
pub fn move_slice(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    id: String,
    task_id: String,
) -> Result<TimeSlice, String> {
    let slice = engine.db().update_slice(UpdateSliceInput {
        id,
        task_id: Some(task_id),
        start: None,
        end: None,
    })?;
    let merged = engine.db().merge_adjacent_same_task(&slice.id)?;
    engine.db().coalesce_near_adjacent()?;
    let out = engine
        .db()
        .get_slice(&merged.id)?
        .unwrap_or(merged);
    let _ = engine.refresh_after_edit()?;
    engine.emit_state(&app);
    Ok(out)
}

#[tauri::command]
pub fn get_db_path(engine: State<'_, Arc<Engine>>) -> Result<String, String> {
    Ok(engine.db().path().display().to_string())
}

#[tauri::command]
pub fn reveal_database(engine: State<'_, Arc<Engine>>, app: AppHandle) -> Result<(), String> {
    let path = engine.db().path().clone();
    let folder = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| path.clone());
    tauri_plugin_opener::open_path(&folder, None::<&str>).map_err(|e| e.to_string())?;
    let _ = app;
    Ok(())
}

#[tauri::command]
pub fn export_database(engine: State<'_, Arc<Engine>>, dest: String) -> Result<(), String> {
    engine.db().backup_to(Path::new(&dest))
}

#[tauri::command]
pub fn import_database(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    source: String,
) -> Result<(), String> {
    let dest = engine.db().path().clone();
    let backup = dest.with_extension("db.bak");
    engine.db().backup_to(&backup)?;
    std::fs::copy(&source, &dest).map_err(|e| e.to_string())?;
    engine.db().reopen()?;
    engine.reset_after_import()?;
    engine.emit_state(&app);
    Ok(())
}

#[tauri::command]
pub fn open_switcher(app: AppHandle) {
    show_switcher(&app);
}

#[tauri::command]
pub fn close_switcher(app: AppHandle) {
    hide_switcher(&app);
}

#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    show_main(&app);
}

#[tauri::command]
pub fn set_tracker_visible(app: AppHandle, visible: bool) {
    crate::engine::set_tracker_visible(&app, visible);
}

#[tauri::command]
pub fn current_week_start() -> String {
    format_day(week_start_monday(today_local()))
}

#[tauri::command]
pub fn shift_week(week_start: String, delta_weeks: i64) -> Result<String, String> {
    let d = parse_day(&week_start)? + Duration::weeks(delta_weeks);
    Ok(format_day(week_start_monday(d)))
}
