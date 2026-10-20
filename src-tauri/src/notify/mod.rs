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
