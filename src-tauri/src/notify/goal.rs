use super::{get_setting_string, set_setting_string, show, ClickAction};
use crate::db::Db;
use crate::domain::TrackerState;
use chrono::Local;
use tauri::AppHandle;

const KEY_DATE: &str = "notify_daily_goal_date";

/// Fire once per local day when the daily goal is first met.
pub fn check_daily_goal(app: &AppHandle, db: &Db, state: &TrackerState) {
    let Ok(settings) = db.get_settings() else {
        return;
    };
    if !settings.notify_daily_goal || !state.goal_met {
        return;
    }
    let today = Local::now().date_naive().format("%Y-%m-%d").to_string();
    let already = get_setting_string(db, KEY_DATE)
        .ok()
        .flatten()
        .unwrap_or_default();
    if already == today {
        return;
    }
    let hours = settings.daily_goal_mins as f64 / 60.0;
    let body = if hours.fract() == 0.0 {
        format!("You've hit {} hours today.", hours as u32)
    } else {
        format!("You've hit your daily goal ({} minutes).", settings.daily_goal_mins)
    };
    show(app, "Daily goal reached", &body, ClickAction::Main);
    let _ = set_setting_string(db, KEY_DATE, &today);
}

/// Pure helper for tests: whether we should notify given stored date and today.
#[cfg(test)]
pub fn should_notify(stored_date: Option<&str>, today: &str, goal_met: bool, enabled: bool) -> bool {
    enabled && goal_met && stored_date != Some(today)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notifies_once_per_day() {
        assert!(should_notify(None, "2026-03-20", true, true));
        assert!(should_notify(Some("2026-03-19"), "2026-03-20", true, true));
        assert!(!should_notify(Some("2026-03-20"), "2026-03-20", true, true));
        assert!(!should_notify(None, "2026-03-20", false, true));
        assert!(!should_notify(None, "2026-03-20", true, false));
    }
}
