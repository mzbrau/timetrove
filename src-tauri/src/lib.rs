mod chrome;
mod commands;
mod db;
mod domain;
mod engine;

#[cfg(test)]
mod test_support;

use chrome::{refresh_tray_tooltip, setup_tray};
use db::open_db;
use engine::{hide_gap_window, hide_switcher, position_tracker, show_switcher, Engine};
use std::time::Duration;
use tauri::{Emitter, Manager, RunEvent, WindowEvent};
use tauri_plugin_autostart::MacosLauncher;
use tauri_plugin_autostart::ManagerExt as AutostartExt;
use tauri_plugin_global_shortcut::{
    Builder as ShortcutBuilder, GlobalShortcutExt, Shortcut, ShortcutState,
};
use tauri_plugin_window_state::{Builder as WindowStateBuilder, StateFlags};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(
            WindowStateBuilder::new()
                // Never restore visibility — gap/switcher are ephemeral overlays
                .with_state_flags(StateFlags::all() - StateFlags::VISIBLE)
                .with_denylist(&["gap", "switcher"])
                .build(),
        )
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            engine::show_main(app);
        }))
        .plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            None,
        ));

    builder = builder.setup(|app| {
        let app_dir = app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?;
        std::fs::create_dir_all(&app_dir).map_err(|e| e.to_string())?;
        let db_path = app_dir.join("time-keeper.db");
        let conn = open_db(&db_path).map_err(|e| e.to_string())?;
        let database = db::Db::new(conn, db_path);
        let engine = Engine::new(database);
        app.manage(engine.clone());

        if let Some(tracker) = app.get_webview_window("tracker") {
            position_tracker(&tracker);
        }

        // Overlays must start hidden; window-state must not reopen them.
        hide_gap_window(app.handle());
        hide_switcher(app.handle());

        if let Some(gap) = app.get_webview_window("gap") {
            gap.on_window_event(|event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
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
        if settings.autostart {
            let _ = app.autolaunch().enable();
        }

        // Idle + midnight poller
        let handle = app.handle().clone();
        let eng = engine.clone();
        std::thread::spawn(move || {
            let mut last_midnight_check = chrono::Local::now().date_naive();
            // Startup gap check shortly after launch
            std::thread::sleep(Duration::from_secs(1));
            eng.on_resume_or_startup_gap(&handle);
            loop {
                std::thread::sleep(Duration::from_secs(5));
                eng.check_idle(&handle);
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
        Ok(())
    });

    let app = builder
        .invoke_handler(tauri::generate_handler![
            commands::get_tracker_state,
            commands::list_tasks,
            commands::create_task,
            commands::update_task,
            commands::start_task,
            commands::stop_tracking,
            commands::switch_task,
            commands::create_and_start,
            commands::resolve_gap_previous,
            commands::resolve_gap_other,
            commands::resolve_gap_discard,
            commands::get_settings,
            commands::update_settings,
            commands::get_timesheet,
            commands::list_slices_for_day,
            commands::create_slice,
            commands::update_slice,
            commands::delete_slice,
            commands::split_slice,
            commands::move_slice,
            commands::get_db_path,
            commands::reveal_database,
            commands::export_database,
            commands::import_database,
            commands::open_switcher,
            commands::close_switcher,
            commands::open_main_window,
            commands::set_tracker_visible,
            commands::current_week_start,
            commands::shift_week,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Time Keeper");

    app.run(|app_handle, event| {
        if let RunEvent::Reopen { .. } = event {
            engine::show_main(app_handle);
        }
    });
}
