use crate::engine::{position_tracker, set_tracker_visible, show_main, Engine};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager,
};

pub fn setup_tray(app: &AppHandle, engine: Arc<Engine>) -> Result<(), String> {
    let show_bar = MenuItem::with_id(app, "show_bar", "Show tracker bar", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let hide_bar = MenuItem::with_id(app, "hide_bar", "Hide tracker bar", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let open_main = MenuItem::with_id(app, "open_main", "Open Time Keeper", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let start_stop = MenuItem::with_id(app, "start_stop", "Start / Stop", true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)
        .map_err(|e| e.to_string())?;

    let menu = Menu::with_items(
        app,
        &[&show_bar, &hide_bar, &open_main, &start_stop, &quit],
    )
    .map_err(|e| e.to_string())?;

    let icon = app
        .default_window_icon()
        .cloned()
        .ok_or_else(|| "Missing default window icon".to_string())?;

    let engine_menu = engine.clone();
    TrayIconBuilder::with_id("main")
        .icon(icon)
        .menu(&menu)
        .tooltip("Time Keeper")
        .on_menu_event(move |app, event| {
            match event.id.as_ref() {
                "show_bar" => {
                    set_tracker_visible(app, true);
                    if let Some(w) = app.get_webview_window("tracker") {
                        position_tracker(&w);
                    }
                }
                "hide_bar" => set_tracker_visible(app, false),
                "open_main" => show_main(app),
                "start_stop" => {
                    if let Ok(s) = engine_menu.snapshot() {
                        if s.tracking {
                            let _ = engine_menu.stop();
                        } else if let Some(prev) = s.previous_task {
                            let _ = engine_menu.start_task(&prev.id);
                        }
                    }
                    engine_menu.emit_state(app);
                    refresh_tray_tooltip(app, &engine_menu);
                }
                "quit" => app.exit(0),
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                show_main(tray.app_handle());
            }
        })
        .build(app)
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn refresh_tray_tooltip(app: &AppHandle, engine: &Engine) {
    if let Ok(state) = engine.snapshot() {
        let text = if let Some(task) = &state.current_task {
            format!("{} · {}", task.name, state.daily_total_display)
        } else {
            format!("Not tracking · {}", state.daily_total_display)
        };
        if let Some(tray) = app.tray_by_id("main") {
            let _ = tray.set_tooltip(Some(&text));
        }
    }
}
