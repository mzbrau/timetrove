use crate::domain::*;
use crate::engine::{
    apply_tracker_always_on_top, apply_tracker_size, ensure_tracker_default_position,
    hide_gap_window, hide_snooze_picker, hide_switcher, set_tracker_visible_persisted, show_main,
    show_snooze_picker, show_switcher, Engine,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
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
    // Flush unreported time to the old Jira key before switching, so history isn't dropped
    // and the new key only receives future work. Rejects when a delta cannot be posted.
    if let Some(raw_key) = input.jira_issue_key.as_ref() {
        if let Some(current) = engine.db().get_task(&input.id)? {
            let new_key = crate::db::normalize_jira_issue_key_input(Some(raw_key))?;
            if new_key != current.jira_issue_key && !current.jira_issue_key.is_empty() {
                crate::jira::flush_before_relink(engine.db(), &current.id)?;
            }
        }
    }
    let task = engine.db().update_task(input)?;
    engine.emit_state(&app);
    Ok(task)
}

#[tauri::command]
pub fn get_task_booking_summary(
    engine: State<'_, Arc<Engine>>,
    task_id: String,
) -> Result<TaskBookingSummary, String> {
    engine.db().task_booking_summary(&task_id)
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
pub fn create_and_resolve_gap_other(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    name: String,
) -> Result<TrackerState, String> {
    let state = engine.create_and_resolve_gap_other(name)?;
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

static JIRA_CONNECTION_TEST_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct JiraConnectionTestEvent {
    phase: String,
    message: String,
}

fn emit_jira_test(app: &AppHandle, phase: &str, message: impl Into<String>) {
    let _ = app.emit(
        "jira-connection-test",
        &JiraConnectionTestEvent {
            phase: phase.into(),
            message: message.into(),
        },
    );
}

/// Starts a Jira connection test on a background thread and emits progress via
/// `jira-connection-test`. Returns immediately so the UI stays responsive.
#[tauri::command]
pub fn start_jira_connection_test(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<(), String> {
    if JIRA_CONNECTION_TEST_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Jira connection test already in progress".into());
    }

    let settings = match engine.db().get_settings() {
        Ok(s) => s,
        Err(e) => {
            JIRA_CONNECTION_TEST_IN_PROGRESS.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    if !settings.jira_enabled {
        JIRA_CONNECTION_TEST_IN_PROGRESS.store(false, Ordering::SeqCst);
        return Err("Enable Jira sync first".into());
    }

    let creds = crate::jira::JiraCredentials {
        site_url: settings.jira_site_url,
        email: settings.jira_email,
        api_token: settings.jira_api_token,
        proxy_url: settings.jira_proxy_url,
    };
    let db = engine.db().clone();
    let use_proxy = !creds.proxy_url.trim().is_empty();

    std::thread::spawn(move || {
        let finish = |phase: &str, message: String| {
            emit_jira_test(&app, phase, message);
            JIRA_CONNECTION_TEST_IN_PROGRESS.store(false, Ordering::SeqCst);
        };

        emit_jira_test(
            &app,
            "validating",
            "Validating site URL and credentials…",
        );

        let connect_msg = if use_proxy {
            "Connecting via proxy…"
        } else {
            "Connecting to Jira…"
        };
        emit_jira_test(&app, "connecting", connect_msg);

        match crate::jira::test_connection(&creds) {
            Ok(name) => {
                emit_jira_test(&app, "verifying", "Verifying account…");
                let _ = db.set_jira_last_error(None);
                finish("succeeded", format!("Connected as {name}"));
            }
            Err(e) => {
                let _ = db.set_jira_last_error(Some(&e));
                finish("failed", e);
            }
        }
    });

    Ok(())
}

fn jira_creds_if_enabled(engine: &Engine) -> Result<crate::jira::JiraCredentials, String> {
    let settings = engine.db().get_settings()?;
    if !settings.jira_enabled {
        return Err("Enable Jira sync first".into());
    }
    if settings.jira_site_url.trim().is_empty()
        || settings.jira_email.trim().is_empty()
        || settings.jira_api_token.trim().is_empty()
    {
        return Err("Configure Jira site URL, email, and API token in Settings".into());
    }
    Ok(crate::jira::JiraCredentials {
        site_url: settings.jira_site_url,
        email: settings.jira_email,
        api_token: settings.jira_api_token,
        proxy_url: settings.jira_proxy_url,
    })
}

/// Validate an issue key against Jira and return live details (does not persist).
#[tauri::command]
pub fn fetch_jira_issue(
    engine: State<'_, Arc<Engine>>,
    issue_key: String,
) -> Result<JiraIssueDetails, String> {
    let key = crate::db::normalize_jira_issue_key_input(Some(&issue_key))?;
    if key.is_empty() {
        return Err("Jira issue key is required".into());
    }
    let creds = jira_creds_if_enabled(&engine)?;
    match crate::jira::fetch_issue(&creds, &key) {
        Ok(details) => {
            let _ = engine.db().set_jira_last_error(None);
            Ok(details)
        }
        Err(e) => {
            let _ = engine.db().set_jira_last_error(Some(&e));
            Err(e)
        }
    }
}

/// Refresh cached details for the task's currently linked issue (no-op if unlinked).
#[tauri::command]
pub fn refresh_jira_issue(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    task_id: String,
) -> Result<Option<JiraIssueDetails>, String> {
    let task = engine
        .db()
        .get_task(&task_id)?
        .ok_or_else(|| "Task not found".to_string())?;
    if task.jira_issue_key.is_empty() {
        return Ok(None);
    }
    let creds = jira_creds_if_enabled(&engine)?;
    match crate::jira::fetch_issue(&creds, &task.jira_issue_key) {
        Ok(details) => {
            engine.db().update_jira_issue_cache(&task_id, &details)?;
            let _ = engine.db().set_jira_last_error(None);
            engine.emit_state(&app);
            Ok(Some(details))
        }
        Err(e) => {
            let _ = engine.db().set_jira_last_error(Some(&e));
            Err(e)
        }
    }
}

#[tauri::command]
pub async fn list_assigned_jira_issues(
    engine: State<'_, Arc<Engine>>,
    force: Option<bool>,
) -> Result<JiraAssignedIssuesResult, String> {
    let db = engine.db().clone();
    let force = force.unwrap_or(false);
    tauri::async_runtime::spawn_blocking(move || crate::jira::list_assigned(&db, force))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("URL is required".into());
    }
    if !(trimmed.starts_with("https://") || trimmed.starts_with("http://")) {
        return Err("Only http(s) URLs can be opened".into());
    }
    tauri_plugin_opener::open_url(trimmed, None::<&str>).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn list_task_jira_history(
    engine: State<'_, Arc<Engine>>,
    task_id: String,
) -> Result<Vec<TaskJiraHistoryEntry>, String> {
    engine.db().list_task_jira_history(&task_id)
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
    if settings.copilot_enabled && settings.copilot_poll_interval_mins < 5 {
        return Err("Copilot poll interval must be at least 5 minutes".into());
    }
    if settings.github_activity_enabled && settings.github_activity_poll_interval_mins < 5 {
        return Err("GitHub activity poll interval must be at least 5 minutes".into());
    }
    // Visibility is controlled by tray / tracker Close — don't clobber from Settings UI.
    let mut settings = settings;
    if let Ok(current) = engine.db().get_settings() {
        settings.tracker_visible = current.tracker_visible;
    }
    settings.copilot_poll_interval_mins = settings.copilot_poll_interval_mins.clamp(5, 120);
    settings.github_activity_poll_interval_mins =
        settings.github_activity_poll_interval_mins.clamp(5, 120);
    if settings.copilot_github_host.trim().is_empty() {
        settings.copilot_github_host = "github.com".into();
    }
    engine.db().update_settings(&settings)?;
    if settings.autostart {
        let _ = app.autolaunch().enable();
    } else {
        let _ = app.autolaunch().disable();
    }
    apply_tracker_always_on_top(&app, settings.tracker_always_on_top);
    apply_tracker_size(&app, &engine);
    engine.emit_state(&app);
    let _ = app.emit("settings-changed", &settings);
    // Kick a Copilot poll when enabling / saving credentials.
    if settings.copilot_enabled && !settings.copilot_api_token.trim().is_empty() {
        let eng = engine.inner().clone();
        let handle = app.clone();
        crate::copilot::spawn_poll(engine.db().clone(), move || {
            eng.emit_state(&handle);
        });
    }
    // Kick a GitHub activity poll when enabling / saving credentials.
    if settings.github_activity_enabled && !settings.copilot_api_token.trim().is_empty() {
        let handle = app.clone();
        crate::github::spawn_poll(engine.db().clone(), move || {
            let _ = handle.emit("github-activity-updated", ());
        });
    }
    Ok(settings)
}

static COPILOT_CONNECTION_TEST_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct CopilotConnectionTestEvent {
    phase: String,
    message: String,
}

fn emit_copilot_test(app: &AppHandle, phase: &str, message: impl Into<String>) {
    let _ = app.emit(
        "copilot-connection-test",
        &CopilotConnectionTestEvent {
            phase: phase.into(),
            message: message.into(),
        },
    );
}

#[tauri::command]
pub fn get_copilot_usage(
    engine: State<'_, Arc<Engine>>,
) -> Result<Option<CopilotUsageSummary>, String> {
    crate::copilot::load_cached_summary(engine.db())
}

#[tauri::command]
pub fn get_copilot_usage_history(
    engine: State<'_, Arc<Engine>>,
    limit: Option<u32>,
) -> Result<Vec<CopilotUsageHistoryPoint>, String> {
    engine
        .db()
        .list_copilot_usage_history(limit.unwrap_or(500))
}

#[tauri::command]
pub async fn refresh_copilot_usage(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<Option<CopilotUsageSummary>, String> {
    let settings = engine.db().get_settings()?;
    if !settings.copilot_enabled {
        return Err("Enable Copilot usage tracking first".into());
    }
    let db = engine.db().clone();
    let eng = engine.inner().clone();
    let result = tauri::async_runtime::spawn_blocking(move || crate::copilot::poll_once(&db))
        .await
        .map_err(|e| e.to_string())?;
    eng.emit_state(&app);
    result
}

#[tauri::command]
pub fn get_github_activity(
    engine: State<'_, Arc<Engine>>,
) -> Result<Option<GithubActivityDigest>, String> {
    crate::github::load_cached_digest(engine.db())
}

#[tauri::command]
pub async fn refresh_github_activity(
    engine: State<'_, Arc<Engine>>,
) -> Result<Option<GithubActivityDigest>, String> {
    let settings = engine.db().get_settings()?;
    if !settings.github_activity_enabled {
        return Err("Enable GitHub activity first".into());
    }
    let db = engine.db().clone();
    tauri::async_runtime::spawn_blocking(move || crate::github::poll_once(&db))
        .await
        .map_err(|e| e.to_string())?
}

/// Starts a Copilot connection test on a background thread; emits `copilot-connection-test`.
#[tauri::command]
pub fn start_copilot_connection_test(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<(), String> {
    if COPILOT_CONNECTION_TEST_IN_PROGRESS
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_err()
    {
        return Err("Copilot connection test already in progress".into());
    }

    let settings = match engine.db().get_settings() {
        Ok(s) => s,
        Err(e) => {
            COPILOT_CONNECTION_TEST_IN_PROGRESS.store(false, Ordering::SeqCst);
            return Err(e);
        }
    };
    if !settings.copilot_enabled {
        COPILOT_CONNECTION_TEST_IN_PROGRESS.store(false, Ordering::SeqCst);
        return Err("Enable Copilot usage tracking first".into());
    }

    let creds = crate::copilot::CopilotCredentials {
        github_host: settings.copilot_github_host,
        api_token: settings.copilot_api_token,
        username: settings.copilot_username,
        proxy_url: settings.copilot_proxy_url,
        monthly_limit_override: settings.copilot_monthly_limit_override,
    };
    let use_proxy = !creds.proxy_url.trim().is_empty();
    let db = engine.db().clone();
    let eng = engine.inner().clone();

    std::thread::spawn(move || {
        let finish = |phase: &str, message: String| {
            emit_copilot_test(&app, phase, message);
            COPILOT_CONNECTION_TEST_IN_PROGRESS.store(false, Ordering::SeqCst);
        };

        emit_copilot_test(
            &app,
            "validating",
            "Validating GitHub host and token…",
        );
        let connect_msg = if use_proxy {
            "Connecting via proxy…"
        } else {
            "Connecting to GitHub…"
        };
        emit_copilot_test(&app, "connecting", connect_msg);

        match crate::copilot::test_connection(&creds) {
            Ok(msg) => {
                emit_copilot_test(&app, "verifying", "Fetching Copilot quota…");
                match crate::copilot::poll_once(&db) {
                    Ok(_) => {
                        eng.emit_state(&app);
                        finish("succeeded", msg);
                    }
                    Err(e) => {
                        let _ = db.set_copilot_last_error(Some(&e));
                        eng.emit_state(&app);
                        finish("failed", e);
                    }
                }
            }
            Err(e) => {
                let _ = db.set_copilot_last_error(Some(&e));
                eng.emit_state(&app);
                finish("failed", e);
            }
        }
    });

    Ok(())
}

#[tauri::command]
pub fn get_timesheet(
    engine: State<'_, Arc<Engine>>,
    week_start: String,
) -> Result<Timesheet, String> {
    engine.timesheet(&week_start)
}

#[tauri::command]
pub fn get_insights(
    engine: State<'_, Arc<Engine>>,
    range_start: String,
    range_end: String,
) -> Result<InsightsReport, String> {
    engine.insights(&range_start, &range_end)
}

#[tauri::command]
pub fn list_slices_for_day(
    engine: State<'_, Arc<Engine>>,
    day: String,
) -> Result<Vec<TimeSlice>, String> {
    let d = parse_day(&day)?;
    let (start, end) = local_day_bounds(d);
    engine.db().list_slices_for_day_display(start, end)
}

#[tauri::command]
pub fn create_slice(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    input: CreateSliceInput,
) -> Result<TimeSlice, String> {
    let note = input.note.as_deref().unwrap_or("");
    let slice = engine
        .db()
        .insert_closed_slice(&input.task_id, input.start, input.end, note)?;
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
        note: None,
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
pub fn coalesce_near_adjacent(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<(), String> {
    engine.db().coalesce_near_adjacent()?;
    let _ = engine.refresh_after_edit()?;
    engine.emit_state(&app);
    Ok(())
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
pub fn start_snooze(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
    until: DateTime<Utc>,
) -> Result<TrackerState, String> {
    let state = engine.start_snooze(until)?;
    hide_snooze_picker(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn clear_snooze(
    engine: State<'_, Arc<Engine>>,
    app: AppHandle,
) -> Result<TrackerState, String> {
    let state = engine.clear_snooze()?;
    hide_snooze_picker(&app);
    engine.emit_state(&app);
    Ok(state)
}

#[tauri::command]
pub fn open_snooze_picker(app: AppHandle) {
    show_snooze_picker(&app);
}

#[tauri::command]
pub fn close_snooze_picker(app: AppHandle) {
    hide_snooze_picker(&app);
}

#[tauri::command]
pub fn open_main_window(app: AppHandle) {
    show_main(&app);
}

#[tauri::command]
pub fn set_tracker_visible(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    visible: bool,
) {
    set_tracker_visible_persisted(&app, &engine, visible);
}

#[tauri::command]
pub fn reset_tracker_position(app: AppHandle, engine: State<'_, Arc<Engine>>) {
    ensure_tracker_default_position(&app, &engine, true);
}

#[tauri::command]
pub fn set_tracker_always_on_top(
    app: AppHandle,
    engine: State<'_, Arc<Engine>>,
    always_on_top: bool,
) -> Result<Settings, String> {
    let mut settings = engine.db().get_settings()?;
    settings.tracker_always_on_top = always_on_top;
    engine.db().update_settings(&settings)?;
    apply_tracker_always_on_top(&app, always_on_top);
    let _ = app.emit("settings-changed", &settings);
    Ok(settings)
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
