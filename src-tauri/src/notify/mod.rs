//! Optional OS notifications (daily goal, GitHub PRs, Jira assigned, Copilot usage).

mod copilot;
mod github;
mod goal;
mod jira;

pub use copilot::check_copilot_usage;
pub use github::check_github_activity;
pub use goal::check_daily_goal;
pub use jira::{jira_poll_interval_secs, spawn_jira_notify_poll};

use crate::engine::show_main;
use serde::Serialize;
use std::sync::atomic::{AtomicI32, Ordering};
use tauri::{AppHandle, Emitter};
use tauri_plugin_notification::NotificationExt;

static NEXT_NOTIFICATION_ID: AtomicI32 = AtomicI32::new(1);

/// Click payload stored in notification `extra`.
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

pub fn any_notify_enabled(settings: &crate::domain::Settings) -> bool {
    settings.notify_daily_goal
        || settings.notify_github_pr
        || settings.notify_jira_assigned
        || settings.notify_copilot_usage
}

pub fn request_permission_if_needed(app: &AppHandle) {
    let _ = app.notification().request_permission();
}

/// Register the tap handler once during app setup.
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

pub fn show(app: &AppHandle, title: &str, body: &str, action: ClickAction) {
    let id = NEXT_NOTIFICATION_ID.fetch_add(1, Ordering::Relaxed);
    let mut builder = app
        .notification()
        .builder()
        .id(id)
        .title(title)
        .body(body);

    match &action {
        ClickAction::Main => {
            builder = builder.extra("kind", "main");
        }
        ClickAction::Url { url } => {
            builder = builder.extra("kind", "url").extra("url", url);
        }
        ClickAction::Tab { tab } => {
            builder = builder.extra("kind", "tab").extra("tab", tab);
        }
    }

    let _ = builder.show();
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
