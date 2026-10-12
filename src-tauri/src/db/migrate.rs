use rusqlite::Connection;
use std::path::Path;

pub fn open_db(path: &Path) -> Result<Connection, String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let conn = Connection::open(path).map_err(|e| e.to_string())?;
    conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL;")
        .map_err(|e| e.to_string())?;
    migrate(&conn)?;
    Ok(conn)
}

fn migrate(conn: &Connection) -> Result<(), String> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            name TEXT NOT NULL UNIQUE COLLATE NOCASE,
            color TEXT NOT NULL,
            description TEXT NOT NULL DEFAULT '',
            notes TEXT NOT NULL DEFAULT '',
            archived INTEGER NOT NULL DEFAULT 0,
            created_at TEXT NOT NULL,
            updated_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS time_slices (
            id TEXT PRIMARY KEY,
            task_id TEXT NOT NULL REFERENCES tasks(id),
            start TEXT NOT NULL,
            end TEXT,
            CHECK (end IS NULL OR end > start)
        );

        CREATE INDEX IF NOT EXISTS idx_slices_start ON time_slices(start);
        CREATE INDEX IF NOT EXISTS idx_slices_task_start ON time_slices(task_id, start);
        CREATE UNIQUE INDEX IF NOT EXISTS idx_one_running
            ON time_slices((1)) WHERE end IS NULL;

        CREATE TABLE IF NOT EXISTS discard_log (
            id TEXT PRIMARY KEY,
            start TEXT NOT NULL,
            end TEXT NOT NULL,
            duration_ms INTEGER NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL
        );
        "#,
    )
    .map_err(|e| e.to_string())?;

    let version: i64 = conn
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if version < 1 {
        let defaults = [
            ("idle_threshold_mins", "10"),
            ("daily_goal_mins", "480"),
            ("switch_shortcut", "CommandOrControl+Shift+T"),
            ("autostart", "true"),
            ("theme", "dark"),
        ];
        for (k, v) in defaults {
            conn.execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                (k, v),
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (1)", [])
            .map_err(|e| e.to_string())?;
    }

    if version < 2 {
        conn.execute(
            "INSERT OR IGNORE INTO settings (key, value) VALUES ('theme', 'dark')",
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (2)", [])
            .map_err(|e| e.to_string())?;
    }

    // v3: marker that near-adjacent coalesce runs once from Db open/reopen hooks
    // (create/update/move keep coalescing; list_slices_for_day stays read-only).
    if version < 3 {
        conn.execute("INSERT INTO schema_migrations (version) VALUES (3)", [])
            .map_err(|e| e.to_string())?;
    }

    if version < 4 {
        conn.execute(
            "ALTER TABLE tasks ADD COLUMN starred INTEGER NOT NULL DEFAULT 0",
            [],
        )
        .map_err(|e| e.to_string())?;
        conn.execute("INSERT INTO schema_migrations (version) VALUES (4)", [])
            .map_err(|e| e.to_string())?;
    }

    if version < 5 {
        let defaults = [
            ("tracker_always_on_top", "true"),
            ("tracker_visible", "true"),
        ];
        for (k, v) in defaults {
            conn.execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                (k, v),
            )
            .map_err(|e| e.to_string())?;
        }
        conn.execute("INSERT INTO schema_migrations (version) VALUES (5)", [])
            .map_err(|e| e.to_string())?;
    }

    // v6: snooze_until is an optional settings key (absent = not snoozing),
    // same pattern as last_good_at — no default row required.
    if version < 6 {
        conn.execute("INSERT INTO schema_migrations (version) VALUES (6)", [])
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
