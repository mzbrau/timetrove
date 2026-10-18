//! Dev-only screenshot runner (macOS).
//!
//! `TIMETROVE_SHOT_PLAN` points at a JSON list of shots and `TIMETROVE_SHOT_OUT` at the
//! output directory. Each shot runs engine actions, emits `demo-shot` so the webviews can
//! set UI state, then captures one window with `screencapture -l <windowNumber>`.

use crate::engine::{
    apply_tracker_size, hide_gap_window, hide_snooze_picker, hide_switcher, show_gap_window,
    show_main, show_snooze_picker, show_switcher, Engine,
};
use serde::Deserialize;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tauri::{AppHandle, Emitter, LogicalSize, Manager};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Shot {
    name: String,
    window: String,
    #[serde(default)]
    actions: Vec<String>,
    #[serde(default)]
    ui: serde_json::Value,
    width: Option<f64>,
    height: Option<f64>,
    #[serde(default = "default_delay")]
    delay_ms: u64,
}

fn default_delay() -> u64 {
    1500
}

pub fn spawn_if_configured(app: AppHandle, engine: Arc<Engine>) {
    let (Ok(plan), Ok(out)) = (
        std::env::var("TIMETROVE_SHOT_PLAN"),
        std::env::var("TIMETROVE_SHOT_OUT"),
    ) else {
        return;
    };
    std::thread::spawn(move || {
        let code = match run(&app, &engine, &plan, PathBuf::from(out)) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("[shots] failed: {e}");
                1
            }
        };
        app.exit(code);
    });
}

fn run(app: &AppHandle, engine: &Arc<Engine>, plan: &str, out: PathBuf) -> Result<(), String> {
    let json = std::fs::read_to_string(plan).map_err(|e| format!("{plan}: {e}"))?;
    let shots: Vec<Shot> = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    // Let webviews boot and load data.
    std::thread::sleep(Duration::from_secs(6));
    let mut failures = 0;
    for shot in &shots {
        for action in &shot.actions {
            apply_action(app, engine, action).map_err(|e| {
                format!("{}: action {action} failed: {e}", shot.name)
            })?;
        }
        engine.emit_state(app);
        let Some(window) = app.get_webview_window(&shot.window) else {
            eprintln!("[shots] {}: no window {}", shot.name, shot.window);
            failures += 1;
            continue;
        };
        if let (Some(w), Some(h)) = (shot.width, shot.height) {
            let _ = window.set_size(LogicalSize::new(w, h));
        }
        // Occluded windows pause requestAnimationFrame, which freezes chart animations.
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        std::thread::sleep(Duration::from_millis(500));
        let _ = app.emit("demo-shot", &shot.ui);
        std::thread::sleep(Duration::from_millis(shot.delay_ms));
        let path = out.join(format!("{}.png", shot.name));
        let result = capture(app, &shot.window, &path);
        if shot.window == "main" {
            let _ = window.set_always_on_top(false);
        }
        match result {
            Ok(()) => println!("[shots] {}", path.display()),
            Err(e) => {
                eprintln!("[shots] {}: {e}", shot.name);
                failures += 1;
            }
        }
    }
    if failures > 0 {
        return Err(format!("{failures} shot(s) failed"));
    }
    Ok(())
}

fn find_task_id(engine: &Engine, name: &str) -> Result<String, String> {
    engine
        .db()
        .list_tasks(true)?
        .into_iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
        .map(|t| t.id)
        .ok_or_else(|| format!("no task named {name}"))
}

fn apply_action(app: &AppHandle, engine: &Arc<Engine>, action: &str) -> Result<(), String> {
    let (verb, arg) = action.split_once(':').unwrap_or((action, ""));
    match verb {
        "track" => {
            let id = find_task_id(engine, arg)?;
            let state = engine.snapshot()?;
            if state.current_task.as_ref().map(|t| t.id.as_str()) != Some(id.as_str()) {
                if state.tracking {
                    engine.switch_task(&id)?;
                } else {
                    engine.start_task(&id)?;
                }
            }
        }
        "stop" => {
            engine.stop()?;
        }
        "gap" => {
            let mins: i64 = arg.parse().map_err(|_| "gap needs minutes".to_string())?;
            engine.begin_gap(chrono::Utc::now() - chrono::Duration::minutes(mins))?;
            engine.emit_state(app);
            show_gap_window(app);
        }
        "resolveGap" => {
            engine.resolve_gap_previous()?;
            hide_gap_window(app);
        }
        "setting" => {
            let (key, value) = arg.split_once('=').ok_or("setting needs key=value")?;
            let mut s = engine.db().get_settings()?;
            match key {
                "daily_goal_mins" => s.daily_goal_mins = value.parse().map_err(|_| "bad number")?,
                "copilot_enabled" => s.copilot_enabled = value == "true",
                "tracker_show_seconds" => s.tracker_show_seconds = value == "true",
                "theme" => s.theme = value.to_string(),
                other => return Err(format!("unsupported setting {other}")),
            }
            engine.db().update_settings(&s)?;
            apply_tracker_size(app, engine);
            let _ = app.emit("settings-changed", &engine.db().get_settings()?);
        }
        "showSwitcher" => show_switcher(app),
        "hideSwitcher" => hide_switcher(app),
        "showSnooze" => show_snooze_picker(app),
        "hideSnooze" => hide_snooze_picker(app),
        "showMain" => show_main(app),
        "hideMain" => {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.hide();
            }
        }
        "wait" => {
            let ms: u64 = arg.parse().unwrap_or(500);
            std::thread::sleep(Duration::from_millis(ms));
        }
        other => return Err(format!("unknown action {other}")),
    }
    engine.emit_state(app);
    Ok(())
}

fn window_number(app: &AppHandle, label: &str) -> Result<isize, String> {
    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("no window {label}"))?;
    let (tx, rx) = std::sync::mpsc::channel();
    app.run_on_main_thread(move || {
        let result = window.ns_window().map_err(|e| e.to_string()).map(|ptr| {
            let obj = unsafe { &*(ptr as *const objc2::runtime::AnyObject) };
            let n: isize = unsafe { objc2::msg_send![obj, windowNumber] };
            n
        });
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv_timeout(Duration::from_secs(5))
        .map_err(|e| e.to_string())?
}

/// Default: snapshot the window's WKWebView (no Screen Recording permission needed).
/// `TIMETROVE_SHOT_CAPTURE=window` uses `screencapture` to include native window chrome.
fn capture(app: &AppHandle, label: &str, path: &std::path::Path) -> Result<(), String> {
    if std::env::var("TIMETROVE_SHOT_CAPTURE").as_deref() == Ok("window") {
        capture_window(app, label, path)
    } else {
        capture_webview(app, label, path)
    }
}

fn capture_webview(app: &AppHandle, label: &str, path: &std::path::Path) -> Result<(), String> {
    use block2::RcBlock;
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use std::sync::Mutex;

    let window = app
        .get_webview_window(label)
        .ok_or_else(|| format!("no window {label}"))?;
    let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<u8>, String>>();
    let tx = Arc::new(Mutex::new(Some(tx)));
    window
        .with_webview(move |wv| {
            let tx = tx.clone();
            let block = RcBlock::new(move |image: *mut AnyObject, _error: *mut AnyObject| {
                let result = if image.is_null() {
                    Err("WKWebView snapshot returned no image".to_string())
                } else {
                    unsafe {
                        let tiff: *mut AnyObject = msg_send![image, TIFFRepresentation];
                        let rep: *mut AnyObject =
                            msg_send![class!(NSBitmapImageRep), imageRepWithData: tiff];
                        let props: *mut AnyObject = msg_send![class!(NSDictionary), dictionary];
                        // NSBitmapImageFileTypePNG = 4
                        let png: *mut AnyObject =
                            msg_send![rep, representationUsingType: 4usize, properties: props];
                        if png.is_null() {
                            Err("PNG encoding failed".to_string())
                        } else {
                            let len: usize = msg_send![png, length];
                            let bytes: *const u8 = msg_send![png, bytes];
                            Ok(std::slice::from_raw_parts(bytes, len).to_vec())
                        }
                    }
                };
                if let Some(tx) = tx.lock().ok().and_then(|mut g| g.take()) {
                    let _ = tx.send(result);
                }
            });
            unsafe {
                let webview = wv.inner() as *mut AnyObject;
                // Width is in points; WebKit renders at the window's backing scale (2x on Retina).
                let config: *mut AnyObject = msg_send![class!(WKSnapshotConfiguration), new];
                let bounds: objc2_rect::CGRect = msg_send![webview, bounds];
                let num: *mut AnyObject =
                    msg_send![class!(NSNumber), numberWithDouble: bounds.size.width];
                let _: () = msg_send![config, setSnapshotWidth: num];
                let _: () = msg_send![webview, takeSnapshotWithConfiguration: config, completionHandler: &*block];            }
        })
        .map_err(|e| e.to_string())?;
    let png = rx
        .recv_timeout(Duration::from_secs(10))
        .map_err(|e| format!("snapshot timed out: {e}"))??;
    std::fs::write(path, png).map_err(|e| e.to_string())
}

/// Minimal CoreGraphics geometry with Objective-C encodings for `msg_send!` returns.
mod objc2_rect {
    use objc2::encode::{Encode, Encoding};

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGPoint {
        pub x: f64,
        pub y: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGSize {
        pub width: f64,
        pub height: f64,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct CGRect {
        pub origin: CGPoint,
        pub size: CGSize,
    }
    unsafe impl Encode for CGPoint {
        const ENCODING: Encoding = Encoding::Struct("CGPoint", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for CGSize {
        const ENCODING: Encoding = Encoding::Struct("CGSize", &[f64::ENCODING, f64::ENCODING]);
    }
    unsafe impl Encode for CGRect {
        const ENCODING: Encoding = Encoding::Struct("CGRect", &[CGPoint::ENCODING, CGSize::ENCODING]);
    }
}

fn capture_window(app: &AppHandle, label: &str, path: &std::path::Path) -> Result<(), String> {
    let id = window_number(app, label)?;
    let status = std::process::Command::new("screencapture")
        .arg("-x")
        .arg("-o")
        .arg(format!("-l{id}"))
        .arg(path)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() || !path.exists() {
        return Err(format!(
            "screencapture failed ({status}); grant Screen Recording permission to the terminal"
        ));
    }
    Ok(())
}
