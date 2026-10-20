//! Optional OS notifications (daily goal, GitHub PRs, Jira assigned, Copilot usage).

mod copilot;
mod github;
mod goal;
mod jira;

pub use copilot::check_copilot_usage;
pub use github::check_github_activity;
pub use goal::check_daily_goal;
pub use jira::{jira_poll_interval_secs, spawn_jira_notify_poll};

use crate::db::Db;
use crate::domain::Settings;
use crate::engine::show_main;
use serde::Serialize;
use std::path::{Path, PathBuf};
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_notification::NotificationExt;

/// Which integration fired the notification — selects the toast image.
#[derive(Debug, Clone, Copy)]
pub enum NotifyKind {
    Goal,
    Github,
    Jira,
    Copilot,
}

impl NotifyKind {
    fn icon_file(self) -> &'static str {
        match self {
            NotifyKind::Goal => "goal.png",
            NotifyKind::Github => "github.png",
            NotifyKind::Jira => "jira.png",
            NotifyKind::Copilot => "copilot.png",
        }
    }
}

/// Click payload for notification taps.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ClickAction {
    /// Focus the main window (no specific tab).
    Main,
    /// Open an external URL (PR or Jira browse link).
    Url { url: String },
    /// Focus main and switch to a tab.
    Tab { tab: String },
}

pub fn any_notify_enabled(settings: &Settings) -> bool {
    settings.notify_daily_goal
        || settings.notify_github_pr
        || settings.notify_jira_assigned
        || settings.notify_copilot_usage
}

fn trim_eq_ci(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

fn trim_eq(a: &str, b: &str) -> bool {
    a.trim() == b.trim()
}

/// Clear notification baselines when alerts are re-enabled or integration identity changes,
/// so the next poll silently re-baselines instead of replaying offline deltas.
pub fn reset_snapshots_for_settings(db: &Db, prev: &Settings, next: &Settings) {
    let github_identity_changed = !trim_eq_ci(&prev.copilot_github_host, &next.copilot_github_host)
        || !trim_eq(&prev.copilot_api_token, &next.copilot_api_token)
        || !trim_eq(&prev.copilot_proxy_url, &next.copilot_proxy_url)
        || !trim_eq_ci(&prev.copilot_username, &next.copilot_username);
    let github_reenabled = (!prev.github_activity_enabled && next.github_activity_enabled)
        || (!prev.notify_github_pr && next.notify_github_pr);
    if github_reenabled || (next.notify_github_pr && github_identity_changed) {
        github::clear_snapshot(db);
    }

    let jira_identity_changed = !trim_eq_ci(&prev.jira_site_url, &next.jira_site_url)
        || !trim_eq_ci(&prev.jira_email, &next.jira_email)
        || !trim_eq(&prev.jira_api_token, &next.jira_api_token);
    let jira_reenabled = (!prev.jira_enabled && next.jira_enabled)
        || (!prev.notify_jira_assigned && next.notify_jira_assigned);
    if jira_reenabled || (next.notify_jira_assigned && jira_identity_changed) {
        jira::clear_snapshot(db);
    }

    let copilot_identity_changed = github_identity_changed; // shares host/token/proxy/username
    let copilot_reenabled = (!prev.copilot_enabled && next.copilot_enabled)
        || (!prev.notify_copilot_usage && next.notify_copilot_usage);
    if copilot_reenabled || (next.notify_copilot_usage && copilot_identity_changed) {
        copilot::clear_fired(db);
    }
}

pub fn request_permission_if_needed(app: &AppHandle) {
    let _ = app.notification().request_permission();
}

/// Register the tap handler once during app setup (plugin lifecycle; own toasts use perform_click).
pub fn register_action_handler(app: &AppHandle) {
    let handle = app.clone();
    let _ = app.notification().on_action(move |performed| {
        if performed.action_id() != "tap" {
            return;
        }
        let Some(notif) = performed.notification() else {
            show_main(&handle);
            return;
        };
        let extra = notif.extra();
        let kind = extra
            .get("kind")
            .and_then(|v| v.as_str())
            .unwrap_or("main");
        match kind {
            "url" => {
                if let Some(url) = extra.get("url").and_then(|v| v.as_str()) {
                    let trimmed = url.trim();
                    if !trimmed.is_empty() {
                        let _ = tauri_plugin_opener::open_url(trimmed, None::<&str>);
                        return;
                    }
                }
                show_main(&handle);
            }
            "tab" => {
                let tab = extra
                    .get("tab")
                    .and_then(|v| v.as_str())
                    .unwrap_or("tasks");
                show_main(&handle);
                let _ = handle.emit("navigate-main-tab", tab);
            }
            _ => {
                show_main(&handle);
            }
        }
    });
}

fn perform_click(app: &AppHandle, action: &ClickAction) {
    match action {
        ClickAction::Main => {
            show_main(app);
        }
        ClickAction::Url { url } => {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                let _ = tauri_plugin_opener::open_url(trimmed, None::<&str>);
            } else {
                show_main(app);
            }
        }
        ClickAction::Tab { tab } => {
            show_main(app);
            let _ = app.emit("navigate-main-tab", tab.as_str());
        }
    }
}

fn resolve_icon_path(app: &AppHandle, kind: NotifyKind) -> Option<PathBuf> {
    let file = kind.icon_file();
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(resource_dir) = app.path().resource_dir() {
        // Bundled layout: resources/notify/<file> (preserving src-tauri relative path)
        candidates.push(resource_dir.join("resources").join("notify").join(file));
        // Flat resource dir copy of the glob matches
        candidates.push(resource_dir.join(file));
        candidates.push(resource_dir.join("notify").join(file));
    }
    candidates.push(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("notify")
            .join(file),
    );
    candidates.into_iter().find(|p| p.is_file())
}

#[allow(unused_variables)]
fn configure_platform(notification: &mut notify_rust::Notification, app: &AppHandle) {
    #[cfg(windows)]
    {
        let identifier = app.config().identifier.clone();
        let sep = std::path::MAIN_SEPARATOR;
        if let Ok(exe) = tauri::utils::platform::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let curr_dir = exe_dir.display().to_string();
                let debug_suffix = format!("{sep}target{sep}debug");
                let release_suffix = format!("{sep}target{sep}release");
                if !(curr_dir.ends_with(&debug_suffix) || curr_dir.ends_with(&release_suffix)) {
                    notification.app_id(&identifier);
                }
            }
        }
    }
    #[cfg(target_os = "macos")]
    {
        let identifier = app.config().identifier.clone();
        let _ = notify_rust::set_application(if tauri::is_dev() {
            "com.apple.Terminal"
        } else {
            &identifier
        });
    }
}

/// Show an OS notification with a per-kind image (via notify-rust `image_path`).
pub fn show(app: &AppHandle, title: &str, body: &str, action: ClickAction, kind: NotifyKind) {
    let mut notification = notify_rust::Notification::new();
    notification.summary(title).body(body);
    if let Some(path) = resolve_icon_path(app, kind) {
        if let Some(s) = path.to_str() {
            notification.image_path(s);
        }
    }
    configure_platform(&mut notification, app);

    let handle = app.clone();
    let _ = std::thread::Builder::new()
        .name("timetrove-notify".into())
        .spawn(move || {
            let Ok(shown) = notification.show() else {
                return;
            };
            let _ = shown.wait_for_response(|response: &notify_rust::NotificationResponse| {
                if matches!(response, notify_rust::NotificationResponse::Default) {
                    perform_click(&handle, &action);
                }
            });
        });
}

/// Format a list of item labels for a notification body.
pub(crate) fn format_item_body(labels: &[String]) -> String {
    match labels.len() {
        0 => String::new(),
        1 => labels[0].clone(),
        n => {
            let mut lines: Vec<String> = labels.iter().take(5).cloned().collect();
            if n > 5 {
                lines.push(format!("…and {} more", n - 5));
            }
            lines.join("\n")
        }
    }
}

pub(crate) fn click_for_items(urls: &[String], multi_tab: &str) -> ClickAction {
    match urls.len() {
        0 => ClickAction::Main,
        1 => ClickAction::Url {
            url: urls[0].clone(),
        },
        _ => ClickAction::Tab {
            tab: multi_tab.to_string(),
        },
    }
}

/// Setting key helpers shared by notify submodules.
pub(crate) fn get_setting_string(db: &crate::db::Db, key: &str) -> Result<Option<String>, String> {
    db.get_setting_value(key)
}

pub(crate) fn set_setting_string(db: &crate::db::Db, key: &str, value: &str) -> Result<(), String> {
    db.set_setting_value(key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_files_exist_in_manifest_resources() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources").join("notify");
        for kind in [
            NotifyKind::Goal,
            NotifyKind::Github,
            NotifyKind::Jira,
            NotifyKind::Copilot,
        ] {
            let path = root.join(kind.icon_file());
            assert!(
                path.is_file(),
                "missing notify icon {}",
                path.display()
            );
        }
    }

    #[test]
    fn resolve_falls_back_to_manifest_dir() {
        // Without an AppHandle we only check the on-disk layout used in tauri dev.
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("notify")
            .join(NotifyKind::Goal.icon_file());
        assert!(path.is_file());
        assert!(Path::new(&path).extension().is_some_and(|e| e == "png"));
    }
}
