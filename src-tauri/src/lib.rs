mod chrome;
mod commands;
mod copilot;
mod db;
mod demo;
mod domain;
mod engine;
mod github;
mod jira;

#[cfg(test)]
mod test_support;

use chrome::{refresh_tray_tooltip, setup_tray};
use db::open_db;
use engine::{
    apply_tracker_always_on_top, apply_tracker_size, ensure_tracker_default_position,
    hide_gap_window, hide_snooze_picker, hide_switcher, reassert_tracker_topmost_if_enabled,
    set_tracker_visible_persisted, show_main, show_switcher, spawn_tracker_topmost_guard,
    Engine,
};
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, GlobalShortcutExt, Shortcut, ShortcutState,
};
use tauri_plugin_window_state::{Builder as WindowStateBuilder, StateFlags};

#[cfg(debug_assertions)]
pub use demo::seed::seed_demo_db;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let demo_mode = demo::enabled();
    let mut builder = tauri::Builder::default().plugin(tauri_plugin_opener::init());
    if !demo_mode {
        builder = builder.plugin(
            WindowStateBuilder::new()
                // Never restore visibility — gap/switcher/snooze are ephemeral overlays
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .with_denylist(&["gap", "switcher", "snooze"])
                .build(),
        );
    }
    builder = builder
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            engine::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ));

    builder = builder.setup(|app| {
        let demo_mode = demo::enabled();
        let app_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        let db_path = demo::db_path_override().unwrap_or_else(|| app_dir.join("timetrove.db"));
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        let database = db::Db::new(conn, db_path);
        if demo_mode {
            // Seeded running slice must not open a startup gap after a slow dev build.
            database.set_last_good_at(Some(chrono::Utc::now()))?;
        }
        let engine = Engine::new(database);
        // Register before any further work so webview invokes can resolve State.
        app.manage(engine.clone());
        // Webviews may already be invoking; signal that managed commands are safe.
        let _ = app.emit("engine-ready", ());
        // Windows: keep HWND_TOPMOST while tracker is visible + AOT (other apps reorder band).
        spawn_tracker_topmost_guard(app.handle().clone());

        // Legacy near-adjacent same-task cleanup (idempotent; not done on day reads).
        engine.db().coalesce_near_adjacent()?;

        // Overlays must start hidden; window-state must not reopen them.
        hide_gap_window(app.handle());
        hide_switcher(app.handle());
        hide_snooze_picker(app.handle());

        if let Some(gap) = app.get_webview_window("gap") {
            gap.on_window_event(|event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                }
            });
        }

        if let Some(snooze) = app.get_webview_window("snooze") {
            let app_snooze = app.handle().clone();
            snooze.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    hide_snooze_picker(&app_snooze);
                }
            });
        }

        // Hide main instead of destroying it so tray / single-instance can reopen.
        if let Some(main) = app.get_webview_window("main") {
            let main_hide = main.clone();
            main.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    let _ = main_hide.hide();
                }
            });
        }

        // Hide tracker instead of destroying; persist closed state for tray restore.
        // Reassert topmost on focus/move so the Windows taskbar cannot cover the bar.
        if let Some(tracker) = app.get_webview_window("tracker") {
            let eng_close = engine.clone();
            let app_close = app.handle().clone();
            tracker.on_window_event(move |event| {
                match event {
                    WindowEvent::CloseRequested { api, .. } => {
                        api.prevent_close();
                        set_tracker_visible_persisted(&app_close, &eng_close, false);
                    }
                    WindowEvent::Focused(_) | WindowEvent::Moved(_) => {
                        reassert_tracker_topmost_if_enabled(&app_close, &eng_close);
                    }
                    _ => {}
                }
            });
        }

        setup_tray(app.handle(), engine.clone())?;

        // Register global shortcut from settings
        let settings = engine.db().get_settings().unwrap_or_default();
        let shortcut: Shortcut = settings
            .switch_shortcut
            .parse()
            .unwrap_or_else(|_| "CommandOrControl+Shift+T".parse().unwrap());

        app.handle().plugin(
            ShortcutBuilder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state == ShortcutState::Pressed {
                        show_switcher(app);
                    }
                })
                .build(),
        )?;

        let _ = app.global_shortcut().register(shortcut);

        // Autostart preference
        if settings.autostart && !demo_mode {
            let _ = app.autolaunch().enable();
        }

        // Idle + midnight + Jira + Copilot poller
        let handle = app.handle().clone();
        let eng = engine.clone();
        std::thread::spawn(move || {
            let mut last_midnight_check = chrono::Local::now().date_naive();
            let mut last_jira_sync = std::time::Instant::now();
            let mut last_copilot_sync = std::time::Instant::now();
            let mut last_github_activity_sync = std::time::Instant::now();
            // Startup gap check shortly after launch
            std::thread::sleep(Duration::from_secs(1));
            if !demo_mode {
                eng.on_resume_or_startup_gap(&handle);
            }
            // First Copilot poll soon after launch when enabled.
            {
                let eng_c = eng.clone();
                let handle_c = handle.clone();
                copilot::spawn_poll(eng.db().clone(), move || {
                    eng_c.emit_state(&handle_c);
                });
            }
            // First GitHub activity poll soon after launch when enabled.
            {
                let handle_g = handle.clone();
                github::spawn_poll(eng.db().clone(), move || {
                    let _ = handle_g.emit("github-activity-updated", ());
                });
            }
            loop {
                std::thread::sleep(Duration::from_secs(5));
                eng.checkpoint_if_due();
                if demo_mode {
                    eng.emit_state(&handle);
                } else {
                    eng.check_idle(&handle);
                }
                refresh_tray_tooltip(&handle, &eng);

                let today = chrono::Local::now().date_naive();
                if today != last_midnight_check {
                    last_midnight_check = today;
                    if let Err(e) = eng.midnight_split() {
                        let _ = handle.emit("db-error", e);
                    }
                    eng.emit_state(&handle);
                }

                // Also split when we cross midnight while same day marker already updated
                // midnight_split is idempotent if slice already after midnight
                let _ = eng.midnight_split();

                if last_jira_sync.elapsed() >= Duration::from_secs(jira::SYNC_INTERVAL_SECS) {
                    last_jira_sync = std::time::Instant::now();
                    jira::spawn_flush_all(eng.db().clone());
                }

                let copilot_interval = Duration::from_secs(copilot::poll_interval_secs(eng.db()));
                if last_copilot_sync.elapsed() >= copilot_interval {
                    last_copilot_sync = std::time::Instant::now();
                    let eng_c = eng.clone();
                    let handle_c = handle.clone();
                    copilot::spawn_poll(eng.db().clone(), move || {
                        eng_c.emit_state(&handle_c);
                    });
                }

                let github_interval =
                    Duration::from_secs(github::poll_interval_secs(eng.db()));
                if last_github_activity_sync.elapsed() >= github_interval {
                    last_github_activity_sync = std::time::Instant::now();
                    let handle_g = handle.clone();
                    github::spawn_poll(eng.db().clone(), move || {
                        let _ = handle_g.emit("github-activity-updated", ());
                    });
                }
            }
        });

        // Tick daily total every 30s while tracking
        let handle2 = app.handle().clone();
        let eng2 = engine.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(30));
            eng2.emit_state(&handle2);
            refresh_tray_tooltip(&handle2, &eng2);
        });

        engine.emit_state(app.handle());
        // Recover feature-tab flags if the first getSettings raced manage().
        // Re-emit engine-ready for listeners that registered after the first emit.
        let _ = app.emit("engine-ready", ());
        if let Ok(current) = engine.db().get_settings() {
            let _ = app.emit("settings-changed", &current);
        }

        // Apply tracker chrome after manage(); windows start hidden in tauri.conf.
        apply_tracker_always_on_top(app.handle(), settings.tracker_always_on_top);
        ensure_tracker_default_position(app.handle(), &engine, false);
        apply_tracker_size(app.handle(), &engine);
        set_tracker_visible_persisted(app.handle(), &engine, settings.tracker_visible);
        show_main(app.handle());

        #[cfg(all(debug_assertions, target_os = "macos"))]
        if demo_mode {
            demo::shots::spawn_if_configured(app.handle().clone(), engine.clone());
        }

        Ok(())
    });

    let app = builder
        .invoke_handler(tauri::generate_handler![
            commands::get_tracker_state,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::get_task_booking_summary,
            commands::start_task,
            commands::stop_tracking,
            commands::switch_task,
            commands::create_and_start,
            commands::resolve_gap_previous,
            commands::resolve_gap_other,
            commands::create_and_resolve_gap_other,
            commands::resolve_gap_discard,
            commands::get_settings,
            commands::update_settings,
            commands::start_jira_connection_test,
            commands::fetch_jira_issue,
            commands::refresh_jira_issue,
            commands::list_assigned_jira_issues,
            commands::open_external_url,
            commands::list_task_jira_history,
            commands::get_copilot_usage,
            commands::get_copilot_usage_history,
            commands::refresh_copilot_usage,
            commands::start_copilot_connection_test,
            commands::get_github_activity,
            commands::refresh_github_activity,
            commands::get_timesheet,
            commands::get_insights,
            commands::list_slices_for_day,
            commands::create_slice,
            commands::update_slice,
            commands::delete_slice,
            commands::split_slice,
            commands::move_slice,
            commands::coalesce_near_adjacent,
            commands::get_db_path,
            commands::reveal_database,
            commands::export_database,
            commands::import_database,
            commands::open_switcher,
            commands::close_switcher,
            commands::start_snooze,
            commands::clear_snooze,
            commands::open_snooze_picker,
            commands::close_snooze_picker,
            commands::open_main_window,
            commands::set_tracker_visible,
            commands::reset_tracker_position,
            commands::set_tracker_always_on_top,
            commands::current_week_start,
            commands::shift_week,
        ])
        .build(tauri::generate_context!())
        .expect("error while building TimeTrove");

    app.run(|app_handle, event| {
        match event {
            // macOS dock-click reopen; variant is not available on other targets.
            #[cfg(target_os = "macos")]
            RunEvent::Reopen { .. } => {
                engine::show_main(app_handle);
            }
            _ => {}
        }
    });
}
