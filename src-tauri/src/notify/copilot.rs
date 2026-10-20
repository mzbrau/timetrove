use super::{get_setting_string, set_setting_string, show, ClickAction};
use crate::db::Db;
use chrono::Local;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

const KEY_MONTH: &str = "copilot_notify_month";
const KEY_FIRED: &str = "copilot_notify_fired";

pub const THRESHOLDS: [u32; 4] = [50, 80, 90, 95];

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopilotFired {
    pub thresholds: Vec<u32>,
}

/// Pure: which thresholds are newly crossed this month.
pub fn newly_crossed(
    percent_used: f64,
    fired: &[u32],
    current_month: &str,
    stored_month: Option<&str>,
) -> Vec<u32> {
    let mut already: Vec<u32> = if stored_month == Some(current_month) {
        fired.to_vec()
    } else {
        Vec::new()
    };
    // Drop thresholds no longer met (usage reset).
    already.retain(|t| percent_used >= *t as f64);

    let mut newly = Vec::new();
    for &t in &THRESHOLDS {
        if percent_used >= t as f64 && !already.contains(&t) {
            newly.push(t);
        }
    }
    newly
}

pub fn merge_fired(previous: &[u32], newly: &[u32], percent_used: f64) -> Vec<u32> {
    let mut out: Vec<u32> = previous
        .iter()
        .copied()
        .filter(|t| percent_used >= *t as f64)
        .collect();
    for &t in newly {
        if !out.contains(&t) {
            out.push(t);
        }
    }
    out.sort_unstable();
    out
}

pub fn check_copilot_usage(app: &AppHandle, db: &Db, percent_used: f64) {
    let Ok(settings) = db.get_settings() else {
        return;
    };
    if !settings.notify_copilot_usage || !settings.copilot_enabled {
        return;
    }

    let month = Local::now().format("%Y-%m").to_string();
    let stored_month = get_setting_string(db, KEY_MONTH).ok().flatten();
    let fired: CopilotFired = get_setting_string(db, KEY_FIRED)
        .ok()
        .flatten()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

    let newly = newly_crossed(
        percent_used,
        &fired.thresholds,
        &month,
        stored_month.as_deref(),
    );
    if newly.is_empty() {
        // Still refresh fired set when month rolls or usage drops.
        let merged = if stored_month.as_deref() == Some(month.as_str()) {
            merge_fired(&fired.thresholds, &[], percent_used)
        } else {
            merge_fired(&[], &[], percent_used)
        };
        let _ = set_setting_string(db, KEY_MONTH, &month);
        if let Ok(json) = serde_json::to_string(&CopilotFired {
            thresholds: merged,
        }) {
            let _ = set_setting_string(db, KEY_FIRED, &json);
        }
        return;
    }

    let body = if newly.len() == 1 {
        format!("Monthly usage has reached {}%.", newly[0])
    } else {
        let list = newly
            .iter()
            .map(|t| format!("{t}%"))
            .collect::<Vec<_>>()
            .join(", ");
        format!("Monthly usage crossed {list}.")
    };
    show(
        app,
        "Copilot usage alert",
        &body,
        ClickAction::Tab {
            tab: "copilot".into(),
        },
    );

    let base = if stored_month.as_deref() == Some(month.as_str()) {
        fired.thresholds
    } else {
        Vec::new()
    };
    let merged = merge_fired(&base, &newly, percent_used);
    let _ = set_setting_string(db, KEY_MONTH, &month);
    if let Ok(json) = serde_json::to_string(&CopilotFired {
        thresholds: merged,
    }) {
        let _ = set_setting_string(db, KEY_FIRED, &json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crosses_multiple_in_one_jump() {
        let newly = newly_crossed(85.0, &[], "2026-03", None);
        assert_eq!(newly, vec![50, 80]);
    }

    #[test]
    fn does_not_refire_same_month() {
        let newly = newly_crossed(85.0, &[50, 80], "2026-03", Some("2026-03"));
        assert!(newly.is_empty());
    }

    #[test]
    fn resets_on_new_month() {
        let newly = newly_crossed(55.0, &[50, 80, 90], "2026-04", Some("2026-03"));
        assert_eq!(newly, vec![50]);
    }

    #[test]
    fn drops_when_usage_resets() {
        let merged = merge_fired(&[50, 80], &[], 40.0);
        assert!(merged.is_empty());
    }
}
