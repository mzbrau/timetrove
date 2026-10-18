//! Dev-only demo mode used to produce documentation screenshots.
//!
//! Everything here is inert in release builds: `enabled()` is constant `false`
//! and the seeder / shot runner are compiled only with `debug_assertions`.

#[cfg(debug_assertions)]
pub mod seed;
#[cfg(all(debug_assertions, target_os = "macos"))]
pub mod shots;

use std::path::PathBuf;

/// `TIMETROVE_DEMO=1` in a debug build: no network, no idle prompts, no autostart changes.
pub fn enabled() -> bool {
    cfg!(debug_assertions) && std::env::var("TIMETROVE_DEMO").map(|v| v == "1").unwrap_or(false)
}

/// `TIMETROVE_DB_PATH` override (debug builds only).
pub fn db_path_override() -> Option<PathBuf> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var("TIMETROVE_DB_PATH")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
}

/// Database path shown in Settings while in demo mode: the normal install location,
/// so screenshots don't leak the developer's checkout path or user name.
pub fn display_db_path() -> String {
    if cfg!(target_os = "windows") {
        r"%APPDATA%\com.mzbrau.timetrove\timetrove.db".into()
    } else {
        "~/Library/Application Support/com.mzbrau.timetrove/timetrove.db".into()
    }
}

/// Assigned Jira issues served instead of a live search while in demo mode.
pub fn jira_fixture() -> Vec<crate::domain::JiraIssueDetails> {
    std::env::var("TIMETROVE_DEMO_JIRA_FIXTURE")
        .ok()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}
