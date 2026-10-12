use crate::domain::*;
use chrono::{DateTime, Duration, Utc};
use parking_lot::Mutex;
use rusqlite::{params, Connection, DatabaseName, OptionalExtension};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use uuid::Uuid;

pub struct Db {
    conn: Mutex<Connection>,
    path: PathBuf,
}

impl Db {
    pub fn new(conn: Connection, path: PathBuf) -> Arc<Self> {
        Arc::new(Self {
            conn: Mutex::new(conn),
            path,
        })
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn with_conn<T>(&self, f: impl FnOnce(&Connection) -> Result<T, String>) -> Result<T, String> {
        let conn = self.conn.lock();
        f(&conn)
    }

    pub fn reopen(&self) -> Result<(), String> {
        let new = super::open_db(&self.path)?;
        {
            let mut guard = self.conn.lock();
            *guard = new;
        }
        self.coalesce_near_adjacent()?;
        Ok(())
    }

    /// Consistent online backup (includes committed WAL pages).
    pub fn backup_to(&self, dest: &Path) -> Result<(), String> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if dest.exists() {
            std::fs::remove_file(dest).map_err(|e| e.to_string())?;
        }
        self.with_conn(|conn| {
            conn.backup(DatabaseName::Main, dest, None)
                .map_err(|e| e.to_string())
        })
    }

    pub fn list_tasks(&self, include_archived: bool) -> Result<Vec<Task>, String> {
        self.with_conn(|conn| {
            let mut stmt = if include_archived {
                conn.prepare(
                    "SELECT id, name, color, description, notes, archived, starred, created_at, updated_at
                     FROM tasks ORDER BY starred DESC, name COLLATE NOCASE",
                )
            } else {
                conn.prepare(
                    "SELECT id, name, color, description, notes, archived, starred, created_at, updated_at
                     FROM tasks WHERE archived = 0 ORDER BY starred DESC, name COLLATE NOCASE",
                )
            }
            .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], map_task)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows)
        })
    }

    pub fn get_task(&self, id: &str) -> Result<Option<Task>, String> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, name, color, description, notes, archived, starred, created_at, updated_at
                 FROM tasks WHERE id = ?1",
                [id],
                map_task,
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    pub fn existing_colors(&self) -> Result<Vec<String>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT color FROM tasks")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| r.get(0))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows)
        })
    }

    pub fn create_task(&self, input: CreateTaskInput) -> Result<Task, String> {
        let name = input.name.trim().to_string();
        if name.is_empty() {
            return Err("Task name is required".into());
        }
        let color = match input.color {
            Some(c) if !c.trim().is_empty() => c,
            _ => next_color(&self.existing_colors()?),
        };
        let now = Utc::now();
        let task = Task {
            id: Uuid::new_v4().to_string(),
            name,
            color,
            description: input.description.unwrap_or_default(),
            notes: input.notes.unwrap_or_default(),
            archived: false,
            starred: false,
            created_at: now,
            updated_at: now,
        };
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO tasks (id, name, color, description, notes, archived, starred, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, ?6, ?7)",
                params![
                    task.id,
                    task.name,
                    task.color,
                    task.description,
                    task.notes,
                    task.created_at.to_rfc3339(),
                    task.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    "A task with that name already exists".into()
                } else {
                    e.to_string()
                }
            })?;
            Ok(())
        })?;
        Ok(task)
    }

    pub fn update_task(&self, input: UpdateTaskInput) -> Result<Task, String> {
        let mut task = self
            .get_task(&input.id)?
            .ok_or_else(|| "Task not found".to_string())?;
        if let Some(name) = input.name {
            let name = name.trim().to_string();
            if name.is_empty() {
                return Err("Task name is required".into());
            }
            task.name = name;
        }
        if let Some(color) = input.color {
            task.color = color;
        }
        if let Some(description) = input.description {
            task.description = description;
        }
        if let Some(notes) = input.notes {
            task.notes = notes;
        }
        if let Some(archived) = input.archived {
            task.archived = archived;
        }
        if let Some(starred) = input.starred {
            task.starred = starred;
        }
        task.updated_at = Utc::now();
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE tasks SET name=?2, color=?3, description=?4, notes=?5, archived=?6, starred=?7, updated_at=?8
                 WHERE id=?1",
                params![
                    task.id,
                    task.name,
                    task.color,
                    task.description,
                    task.notes,
                    task.archived as i32,
                    task.starred as i32,
                    task.updated_at.to_rfc3339(),
                ],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    "A task with that name already exists".into()
                } else {
                    e.to_string()
                }
            })?;
            Ok(())
        })?;
        Ok(task)
    }

    pub fn running_slice(&self) -> Result<Option<TimeSlice>, String> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, task_id, start, end FROM time_slices WHERE end IS NULL LIMIT 1",
                [],
                map_slice,
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    pub fn get_slice(&self, id: &str) -> Result<Option<TimeSlice>, String> {
        self.with_conn(|conn| {
            conn.query_row(
                "SELECT id, task_id, start, end FROM time_slices WHERE id = ?1",
                [id],
                map_slice,
            )
            .optional()
            .map_err(|e| e.to_string())
        })
    }

    pub fn close_slice(&self, id: &str, end: DateTime<Utc>) -> Result<(), String> {
        self.with_conn(|conn| {
            let end = Self::clamp_close_end(conn, id, end)?;
            let changed = conn
                .execute(
                    "UPDATE time_slices SET end = ?2 WHERE id = ?1 AND end IS NULL",
                    params![id, end.to_rfc3339()],
                )
                .map_err(|e| e.to_string())?;
            if changed == 0 {
                return Err("No running slice to close".into());
            }
            Ok(())
        })
    }

    /// Clamp a running slice's close time so it cannot overrun a later slice that
    /// was inserted while tracking (allowed by COALESCE(end, now) overlap checks).
    fn clamp_close_end(
        conn: &Connection,
        id: &str,
        requested_end: DateTime<Utc>,
    ) -> Result<DateTime<Utc>, String> {
        let start_s: Option<String> = conn
            .query_row(
                "SELECT start FROM time_slices WHERE id = ?1 AND end IS NULL",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        let Some(start_s) = start_s else {
            return Err("No running slice to close".into());
        };
        let start = DateTime::parse_from_rfc3339(&start_s)
            .map_err(|e| e.to_string())?
            .with_timezone(&Utc);

        let bound_s: Option<String> = conn
            .query_row(
                "SELECT start FROM time_slices
                 WHERE id != ?1
                   AND start > ?2
                   AND start < ?3
                 ORDER BY start ASC
                 LIMIT 1",
                params![id, start.to_rfc3339(), requested_end.to_rfc3339()],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;

        let end = if let Some(bound_s) = bound_s {
            let bound = DateTime::parse_from_rfc3339(&bound_s)
                .map_err(|e| e.to_string())?
                .with_timezone(&Utc);
            requested_end.min(bound)
        } else {
            requested_end
        };

        if end <= start {
            return Err("Cannot close: end would not be after start".into());
        }
        Ok(end)
    }

    /// Close an optional running slice and open a replacement in one transaction.
    pub fn replace_running_slice(
        &self,
        old_id: Option<&str>,
        task_id: &str,
        now: DateTime<Utc>,
    ) -> Result<TimeSlice, String> {
        let slice = TimeSlice {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            start: now,
            end: None,
        };
        self.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| e.to_string())?;
            let result = (|| {
                if let Some(id) = old_id {
                    let end = Self::clamp_close_end(conn, id, now)?;
                    let changed = conn
                        .execute(
                            "UPDATE time_slices SET end = ?2 WHERE id = ?1 AND end IS NULL",
                            params![id, end.to_rfc3339()],
                        )
                        .map_err(|e| e.to_string())?;
                    if changed == 0 {
                        return Err("No running slice to close".into());
                    }
                } else {
                    let existing: Option<String> = conn
                        .query_row(
                            "SELECT id FROM time_slices WHERE end IS NULL LIMIT 1",
                            [],
                            |r| r.get(0),
                        )
                        .optional()
                        .map_err(|e| e.to_string())?;
                    if existing.is_some() {
                        return Err("A task is already being tracked".into());
                    }
                }
                conn.execute(
                    "INSERT INTO time_slices (id, task_id, start, end) VALUES (?1, ?2, ?3, NULL)",
                    params![slice.id, slice.task_id, slice.start.to_rfc3339()],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
                    Ok(())
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })?;
        Ok(slice)
    }

    pub fn insert_closed_slice(
        &self,
        task_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<TimeSlice, String> {
        if end <= start {
            return Err("End must be after start".into());
        }
        self.ensure_no_overlap(None, start, end)?;
        let slice = TimeSlice {
            id: Uuid::new_v4().to_string(),
            task_id: task_id.to_string(),
            start,
            end: Some(end),
        };
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO time_slices (id, task_id, start, end) VALUES (?1, ?2, ?3, ?4)",
                params![
                    slice.id,
                    slice.task_id,
                    slice.start.to_rfc3339(),
                    end.to_rfc3339()
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(slice)
    }

    pub fn list_slices_between(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<TimeSlice>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, task_id, start, end FROM time_slices
                     WHERE start < ?2 AND (end IS NULL OR end > ?1)
                     ORDER BY start",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![start.to_rfc3339(), end.to_rfc3339()], map_slice)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows)
        })
    }

    pub fn list_slices_for_day_display(
        &self,
        day_start: DateTime<Utc>,
        day_end: DateTime<Utc>,
    ) -> Result<Vec<TimeSlice>, String> {
        self.list_slices_between(day_start, day_end)
    }

    pub fn delete_slice(&self, id: &str) -> Result<(), String> {
        self.with_conn(|conn| {
            let n = conn
                .execute("DELETE FROM time_slices WHERE id = ?1", [id])
                .map_err(|e| e.to_string())?;
            if n == 0 {
                return Err("Slice not found".into());
            }
            Ok(())
        })
    }

    pub fn update_slice(&self, input: UpdateSliceInput) -> Result<TimeSlice, String> {
        let mut slice = self
            .get_slice(&input.id)?
            .ok_or_else(|| "Slice not found".to_string())?;
        if let Some(task_id) = input.task_id {
            slice.task_id = task_id;
        }
        if let Some(start) = input.start {
            slice.start = start;
        }
        if let Some(end) = input.end {
            slice.end = Some(end);
        }
        let end = slice.end.ok_or_else(|| "Cannot clear end on update; stop tracking first".to_string())?;
        if end <= slice.start {
            return Err("End must be after start".into());
        }
        self.ensure_no_overlap(Some(&slice.id), slice.start, end)?;
        self.with_conn(|conn| {
            conn.execute(
                "UPDATE time_slices SET task_id=?2, start=?3, end=?4 WHERE id=?1",
                params![
                    slice.id,
                    slice.task_id,
                    slice.start.to_rfc3339(),
                    end.to_rfc3339()
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        slice.end = Some(end);
        Ok(slice)
    }

    /// Merge closed slices that touch or nearly touch this one and share the same task.
    pub fn merge_adjacent_same_task(&self, id: &str) -> Result<TimeSlice, String> {
        let mut current_id = id.to_string();
        loop {
            let slice = self
                .get_slice(&current_id)?
                .ok_or_else(|| "Slice not found".to_string())?;
            let Some(end) = slice.end else {
                return Ok(slice);
            };

            if let Some(prev) = self.find_near_prev(&slice.task_id, slice.start)? {
                self.merge_pair(&prev.id, &slice.id, prev.start, end)?;
                current_id = prev.id;
                continue;
            }

            if let Some(next) = self.find_near_next(&slice.task_id, end)? {
                let next_end = next
                    .end
                    .ok_or_else(|| "Cannot merge with a running slice".to_string())?;
                self.merge_pair(&slice.id, &next.id, slice.start, next_end)?;
                continue;
            }

            return Ok(slice);
        }
    }

    /// Merge all same-task closed pairs with a gap under one minute until stable.
    pub fn coalesce_near_adjacent(&self) -> Result<(), String> {
        loop {
            let Some((earlier, later)) = self.find_any_near_pair()? else {
                return Ok(());
            };
            let later_end = later
                .end
                .ok_or_else(|| "Cannot merge a running slice".to_string())?;
            self.merge_pair(&earlier.id, &later.id, earlier.start, later_end)?;
        }
    }

    fn near_gap(earlier_end: DateTime<Utc>, later_start: DateTime<Utc>) -> bool {
        let gap = later_start - earlier_end;
        gap >= chrono::Duration::zero() && gap < chrono::Duration::seconds(60)
    }

    fn find_near_prev(
        &self,
        task_id: &str,
        start: DateTime<Utc>,
    ) -> Result<Option<TimeSlice>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, task_id, start, end FROM time_slices
                     WHERE task_id = ?1 AND end IS NOT NULL",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![task_id], map_slice)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .filter(|s| s.end.is_some_and(|e| Self::near_gap(e, start)))
                .max_by_key(|s| s.end))
        })
    }

    fn find_near_next(
        &self,
        task_id: &str,
        end: DateTime<Utc>,
    ) -> Result<Option<TimeSlice>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, task_id, start, end FROM time_slices
                     WHERE task_id = ?1 AND end IS NOT NULL",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map(params![task_id], map_slice)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            Ok(rows
                .into_iter()
                .filter(|s| Self::near_gap(end, s.start))
                .min_by_key(|s| s.start))
        })
    }

    fn find_any_near_pair(&self) -> Result<Option<(TimeSlice, TimeSlice)>, String> {
        self.with_conn(|conn| {
            let mut stmt = conn
                .prepare(
                    "SELECT id, task_id, start, end FROM time_slices
                     WHERE end IS NOT NULL
                     ORDER BY start",
                )
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], map_slice)
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for i in 0..rows.len() {
                for j in (i + 1)..rows.len() {
                    if rows[i].task_id != rows[j].task_id {
                        continue;
                    }
                    let earlier_end = rows[i].end.expect("closed");
                    if Self::near_gap(earlier_end, rows[j].start) {
                        return Ok(Some((rows[i].clone(), rows[j].clone())));
                    }
                }
            }
            Ok(None)
        })
    }

    fn merge_pair(
        &self,
        keep_id: &str,
        drop_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), String> {
        if end <= start {
            return Err("Merged range invalid".into());
        }
        self.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| e.to_string())?;
            let result = (|| {
                let deleted = conn
                    .execute("DELETE FROM time_slices WHERE id = ?1", [drop_id])
                    .map_err(|e| e.to_string())?;
                if deleted == 0 {
                    return Err("Slice to merge not found".into());
                }
                let updated = conn
                    .execute(
                        "UPDATE time_slices SET start = ?2, end = ?3 WHERE id = ?1",
                        params![keep_id, start.to_rfc3339(), end.to_rfc3339()],
                    )
                    .map_err(|e| e.to_string())?;
                if updated == 0 {
                    return Err("Survivor slice not found".into());
                }
                Ok(())
            })();
            match result {
                Ok(()) => {
                    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
                    Ok(())
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })
    }

    pub fn split_slice(&self, input: SplitSliceInput) -> Result<(TimeSlice, TimeSlice), String> {
        let slice = self
            .get_slice(&input.id)?
            .ok_or_else(|| "Slice not found".to_string())?;
        let end = slice
            .end
            .ok_or_else(|| "Cannot split a running slice".to_string())?;
        if input.at <= slice.start || input.at >= end {
            return Err("Split point must be inside the slice".into());
        }
        // Second half must not collide with anything other than the slice being split.
        self.ensure_no_overlap(Some(&slice.id), input.at, end)?;
        let second_id = Uuid::new_v4().to_string();
        self.with_conn(|conn| {
            conn.execute_batch("BEGIN IMMEDIATE")
                .map_err(|e| e.to_string())?;
            let result = (|| {
                conn.execute(
                    "UPDATE time_slices SET end = ?2 WHERE id = ?1",
                    params![slice.id, input.at.to_rfc3339()],
                )
                .map_err(|e| e.to_string())?;
                conn.execute(
                    "INSERT INTO time_slices (id, task_id, start, end) VALUES (?1, ?2, ?3, ?4)",
                    params![
                        second_id,
                        slice.task_id,
                        input.at.to_rfc3339(),
                        end.to_rfc3339()
                    ],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })();
            match result {
                Ok(()) => {
                    conn.execute_batch("COMMIT").map_err(|e| e.to_string())?;
                    Ok(())
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    Err(e)
                }
            }
        })?;
        let first = self
            .get_slice(&slice.id)?
            .ok_or_else(|| "Slice missing after split".to_string())?;
        let second = self
            .get_slice(&second_id)?
            .ok_or_else(|| "Second slice missing after split".to_string())?;
        Ok((first, second))
    }

    fn ensure_no_overlap(
        &self,
        exclude_id: Option<&str>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), String> {
        // Running slices (end NULL) only occupy through "now", not forever.
        let now = Utc::now().to_rfc3339();
        self.with_conn(|conn| {
            let sql = if exclude_id.is_some() {
                "SELECT id FROM time_slices
                 WHERE id != ?3
                   AND start < ?2
                   AND COALESCE(end, ?4) > ?1
                 LIMIT 1"
            } else {
                "SELECT id FROM time_slices
                 WHERE start < ?2
                   AND COALESCE(end, ?3) > ?1
                 LIMIT 1"
            };
            let mut stmt = conn.prepare(sql).map_err(|e| e.to_string())?;
            let found = if let Some(id) = exclude_id {
                stmt.query_row(
                    params![start.to_rfc3339(), end.to_rfc3339(), id, now],
                    |r| r.get::<_, String>(0),
                )
            } else {
                stmt.query_row(params![start.to_rfc3339(), end.to_rfc3339(), now], |r| {
                    r.get::<_, String>(0)
                })
            }
            .optional()
            .map_err(|e| e.to_string())?;
            if found.is_some() {
                return Err("Time range overlaps an existing entry".into());
            }
            Ok(())
        })
    }

    pub fn insert_discard(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<DiscardLogEntry, String> {
        let entry = DiscardLogEntry {
            id: Uuid::new_v4().to_string(),
            start,
            end,
            duration_ms: (end - start).num_milliseconds().max(0),
            created_at: Utc::now(),
        };
        self.with_conn(|conn| {
            conn.execute(
                "INSERT INTO discard_log (id, start, end, duration_ms, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![
                    entry.id,
                    entry.start.to_rfc3339(),
                    entry.end.to_rfc3339(),
                    entry.duration_ms,
                    entry.created_at.to_rfc3339(),
                ],
            )
            .map_err(|e| e.to_string())?;
            Ok(())
        })?;
        Ok(entry)
    }

    pub fn get_settings(&self) -> Result<Settings, String> {
        self.with_conn(|conn| {
            let mut settings = Settings::default();
            let mut stmt = conn
                .prepare("SELECT key, value FROM settings")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            for (k, v) in rows {
                match k.as_str() {
                    "idle_threshold_mins" => {
                        settings.idle_threshold_mins = v.parse().unwrap_or(10);
                    }
                    "daily_goal_mins" => {
                        settings.daily_goal_mins = v.parse().unwrap_or(480);
                    }
                    "switch_shortcut" => settings.switch_shortcut = v,
                    "autostart" => settings.autostart = v == "true" || v == "1",
                    "theme" => {
                        settings.theme = if v == "light" {
                            "light".into()
                        } else {
                            "dark".into()
                        };
                    }
                    "tracker_always_on_top" => {
                        settings.tracker_always_on_top = v == "true" || v == "1";
                    }
                    "tracker_visible" => {
                        settings.tracker_visible = v == "true" || v == "1";
                    }
                    _ => {}
                }
            }
            Ok(settings)
        })
    }

    pub fn update_settings(&self, settings: &Settings) -> Result<(), String> {
        self.with_conn(|conn| {
            let theme = if settings.theme == "light" {
                "light"
            } else {
                "dark"
            };
            let pairs = [
                (
                    "idle_threshold_mins",
                    settings.idle_threshold_mins.to_string(),
                ),
                ("daily_goal_mins", settings.daily_goal_mins.to_string()),
                ("switch_shortcut", settings.switch_shortcut.clone()),
                (
                    "autostart",
                    if settings.autostart {
                        "true".into()
                    } else {
                        "false".into()
                    },
                ),
                ("theme", theme.into()),
                (
                    "tracker_always_on_top",
                    if settings.tracker_always_on_top {
                        "true".into()
                    } else {
                        "false".into()
                    },
                ),
                (
                    "tracker_visible",
                    if settings.tracker_visible {
                        "true".into()
                    } else {
                        "false".into()
                    },
                ),
            ];
            for (k, v) in pairs {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![k, v],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    /// Whether the Windows/macOS default tracker dock position was applied once.
    pub fn tracker_default_position_applied(&self) -> Result<bool, String> {
        self.with_conn(|conn| {
            let value: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'tracker_default_position_applied'",
                    [],
                    |r| r.get(0),
                )
                .ok();
            Ok(matches!(value.as_deref(), Some("true") | Some("1")))
        })
    }

    pub fn set_tracker_default_position_applied(&self, applied: bool) -> Result<(), String> {
        self.with_conn(|conn| {
            if applied {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('tracker_default_position_applied', 'true')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [],
                )
                .map_err(|e| e.to_string())?;
            } else {
                conn.execute(
                    "DELETE FROM settings WHERE key = 'tracker_default_position_applied'",
                    [],
                )
                .map_err(|e| e.to_string())?;
            }
            Ok(())
        })
    }

    /// Persisted tracking heartbeat (not part of user-facing Settings).
    pub fn get_last_good_at(&self) -> Result<Option<DateTime<Utc>>, String> {
        self.with_conn(|conn| {
            let value: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'last_good_at'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match value {
                Some(s) if !s.is_empty() => {
                    let dt = DateTime::parse_from_rfc3339(&s)
                        .map_err(|e| e.to_string())?
                        .with_timezone(&Utc);
                    Ok(Some(dt))
                }
                _ => Ok(None),
            }
        })
    }

    pub fn set_last_good_at(&self, at: Option<DateTime<Utc>>) -> Result<(), String> {
        self.with_conn(|conn| match at {
            Some(dt) => {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('last_good_at', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![dt.to_rfc3339()],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            None => {
                conn.execute("DELETE FROM settings WHERE key = 'last_good_at'", [])
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
        })
    }

    /// Persisted snooze deadline (not part of user-facing Settings).
    pub fn get_snooze_until(&self) -> Result<Option<DateTime<Utc>>, String> {
        self.with_conn(|conn| {
            let value: Option<String> = conn
                .query_row(
                    "SELECT value FROM settings WHERE key = 'snooze_until'",
                    [],
                    |r| r.get(0),
                )
                .optional()
                .map_err(|e| e.to_string())?;
            match value {
                Some(s) if !s.is_empty() => {
                    let dt = DateTime::parse_from_rfc3339(&s)
                        .map_err(|e| e.to_string())?
                        .with_timezone(&Utc);
                    Ok(Some(dt))
                }
                _ => Ok(None),
            }
        })
    }

    pub fn set_snooze_until(&self, until: Option<DateTime<Utc>>) -> Result<(), String> {
        self.with_conn(|conn| match until {
            Some(dt) => {
                conn.execute(
                    "INSERT INTO settings (key, value) VALUES ('snooze_until', ?1)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    params![dt.to_rfc3339()],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            }
            None => {
                conn.execute("DELETE FROM settings WHERE key = 'snooze_until'", [])
                    .map_err(|e| e.to_string())?;
                Ok(())
            }
        })
    }

    pub fn daily_total_ms(&self, now: DateTime<Utc>) -> Result<i64, String> {
        let day = now.with_timezone(&chrono::Local).date_naive();
        let (start, end) = local_day_bounds(day);
        let slices = self.list_slices_between(start, end)?;
        let mut total = 0i64;
        for s in slices {
            let s_end = s.end.unwrap_or(now);
            total += overlap_ms(s.start, s_end, start, end);
        }
        Ok(total)
    }

    /// Monday-week booked totals for a task (this week + last week) and last activity.
    pub fn task_booking_summary(&self, task_id: &str) -> Result<TaskBookingSummary, String> {
        if self.get_task(task_id)?.is_none() {
            return Err("Task not found".into());
        }

        let this_monday = week_start_monday(today_local());
        let last_monday = this_monday - Duration::days(7);
        let (this_start, _) = local_day_bounds(this_monday);
        let (next_start, _) = local_day_bounds(this_monday + Duration::days(7));
        let (last_start, _) = local_day_bounds(last_monday);
        let now = Utc::now();

        let sum_for = |slices: &[TimeSlice], w0: DateTime<Utc>, w1: DateTime<Utc>| -> i64 {
            let mut ms = 0i64;
            for s in slices {
                if s.task_id != task_id {
                    continue;
                }
                let end = s.end.unwrap_or(now);
                ms += overlap_ms(s.start, end, w0, w1);
            }
            ms
        };

        let this_slices = self.list_slices_between(this_start, next_start)?;
        let last_slices = self.list_slices_between(last_start, this_start)?;
        let this_week_ms = sum_for(&this_slices, this_start, next_start);
        let last_week_ms = sum_for(&last_slices, last_start, this_start);
        let last_booked_at = self.last_booked_at(task_id)?;

        Ok(TaskBookingSummary {
            this_week_ms,
            this_week_display: format_hhmm(this_week_ms),
            this_week_start: format_day(this_monday),
            last_week_ms,
            last_week_display: format_hhmm(last_week_ms),
            last_week_start: format_day(last_monday),
            last_booked_at,
        })
    }

    fn last_booked_at(&self, task_id: &str) -> Result<Option<DateTime<Utc>>, String> {
        self.with_conn(|conn| {
            let s: Option<String> = conn
                .query_row(
                    "SELECT MAX(COALESCE(end, start)) FROM time_slices WHERE task_id = ?1",
                    [task_id],
                    |r| r.get(0),
                )
                .map_err(|e| e.to_string())?;
            match s {
                Some(raw) => Ok(Some(parse_dt(&raw).map_err(|e| e.to_string())?)),
                None => Ok(None),
            }
        })
    }
}

fn map_task(row: &rusqlite::Row<'_>) -> rusqlite::Result<Task> {
    Ok(Task {
        id: row.get(0)?,
        name: row.get(1)?,
        color: row.get(2)?,
        description: row.get(3)?,
        notes: row.get(4)?,
        archived: row.get::<_, i32>(5)? != 0,
        starred: row.get::<_, i32>(6)? != 0,
        created_at: parse_dt(&row.get::<_, String>(7)?)?,
        updated_at: parse_dt(&row.get::<_, String>(8)?)?,
    })
}

fn map_slice(row: &rusqlite::Row<'_>) -> rusqlite::Result<TimeSlice> {
    let end: Option<String> = row.get(3)?;
    Ok(TimeSlice {
        id: row.get(0)?,
        task_id: row.get(1)?,
        start: parse_dt(&row.get::<_, String>(2)?)?,
        end: end.map(|s| parse_dt(&s)).transpose()?,
    })
}

fn parse_dt(s: &str) -> rusqlite::Result<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(s)
        .map(|d| d.with_timezone(&Utc))
        .map_err(|e| rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::test_db;
    use chrono::{Duration, TimeZone};

    fn sample_task(db: &Db, name: &str) -> Task {
        db.create_task(CreateTaskInput {
            name: name.into(),
            color: Some("#E4572E".into()),
            description: None,
            notes: None,
        })
        .unwrap()
    }

    #[test]
    fn migrate_defaults() {
        let (_dir, db) = test_db();
        let s = db.get_settings().unwrap();
        assert_eq!(s.idle_threshold_mins, 10);
        assert_eq!(s.daily_goal_mins, 480);
        assert_eq!(s.theme, "dark");
        assert!(s.tracker_always_on_top);
        assert!(s.tracker_visible);
    }

    #[test]
    fn task_crud_and_unique_name() {
        let (_dir, db) = test_db();
        let t = sample_task(&db, "Alpha");
        assert_eq!(db.get_task(&t.id).unwrap().unwrap().name, "Alpha");
        assert_eq!(db.list_tasks(false).unwrap().len(), 1);

        let err = db
            .create_task(CreateTaskInput {
                name: "alpha".into(),
                color: None,
                description: None,
                notes: None,
            })
            .unwrap_err();
        assert!(err.contains("already exists"));

        let updated = db
            .update_task(UpdateTaskInput {
                id: t.id.clone(),
                name: Some("Beta".into()),
                color: None,
                description: Some("desc".into()),
                notes: None,
                archived: Some(true),
                starred: None,
            })
            .unwrap();
        assert_eq!(updated.name, "Beta");
        assert!(updated.archived);
        assert!(!updated.starred);
        assert_eq!(db.list_tasks(false).unwrap().len(), 0);
        assert_eq!(db.list_tasks(true).unwrap().len(), 1);
    }

    #[test]
    fn starred_tasks_sort_before_alpha() {
        let (_dir, db) = test_db();
        let zebra = sample_task(&db, "Zebra");
        let _alpha = sample_task(&db, "Alpha");
        let _beta = sample_task(&db, "Beta");
        db.update_task(UpdateTaskInput {
            id: zebra.id.clone(),
            name: None,
            color: None,
            description: None,
            notes: None,
            archived: None,
            starred: Some(true),
        })
        .unwrap();

        let names: Vec<_> = db
            .list_tasks(false)
            .unwrap()
            .into_iter()
            .map(|t| t.name)
            .collect();
        assert_eq!(names, vec!["Zebra", "Alpha", "Beta"]);
    }

    #[test]
    fn task_booking_summary_weeks_and_last_booked() {
        let (_dir, db) = test_db();
        let task = sample_task(&db, "Booked");
        let idle = sample_task(&db, "Idle");

        let this_monday = week_start_monday(today_local());
        let last_monday = this_monday - Duration::days(7);
        let (this_start, _) = local_day_bounds(this_monday);
        let (last_start, _) = local_day_bounds(last_monday);

        db.insert_closed_slice(
            &task.id,
            this_start + Duration::hours(9),
            this_start + Duration::hours(11),
        )
        .unwrap();
        db.insert_closed_slice(
            &task.id,
            last_start + Duration::hours(10),
            last_start + Duration::hours(11) + Duration::minutes(30),
        )
        .unwrap();

        let summary = db.task_booking_summary(&task.id).unwrap();
        assert_eq!(summary.this_week_ms, 2 * 3_600_000);
        assert_eq!(summary.this_week_display, "02:00");
        assert_eq!(summary.last_week_ms, 90 * 60_000);
        assert_eq!(summary.last_week_display, "01:30");
        assert!(summary.last_booked_at.is_some());

        let old_end = this_start - Duration::days(20);
        db.insert_closed_slice(
            &idle.id,
            old_end - Duration::hours(1),
            old_end,
        )
        .unwrap();
        let idle_summary = db.task_booking_summary(&idle.id).unwrap();
        assert_eq!(idle_summary.this_week_ms, 0);
        assert_eq!(idle_summary.last_week_ms, 0);
        assert_eq!(idle_summary.last_booked_at, Some(old_end));

        let never = sample_task(&db, "Never");
        let never_summary = db.task_booking_summary(&never.id).unwrap();
        assert_eq!(never_summary.this_week_ms, 0);
        assert_eq!(never_summary.last_week_ms, 0);
        assert!(never_summary.last_booked_at.is_none());
    }

    #[test]
    fn replace_running_slice_single_open() {
        let (_dir, db) = test_db();
        let a = sample_task(&db, "A");
        let b = sample_task(&db, "B");
        let now = Utc::now();
        let s1 = db.replace_running_slice(None, &a.id, now).unwrap();
        assert!(db.running_slice().unwrap().is_some());

        let s2 = db
            .replace_running_slice(Some(&s1.id), &b.id, now + Duration::minutes(5))
            .unwrap();
        let running = db.running_slice().unwrap().unwrap();
        assert_eq!(running.id, s2.id);
        assert_eq!(running.task_id, b.id);

        let closed = db.get_slice(&s1.id).unwrap().unwrap();
        assert!(closed.end.is_some());

        let err = db
            .replace_running_slice(None, &a.id, now + Duration::minutes(10))
            .unwrap_err();
        assert!(err.contains("already being tracked"));
    }

    #[test]
    fn insert_closed_rejects_overlap() {
        let (_dir, db) = test_db();
        let t = sample_task(&db, "Work");
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        db.insert_closed_slice(&t.id, start, end).unwrap();

        let err = db
            .insert_closed_slice(
                &t.id,
                Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap(),
            )
            .unwrap_err();
        assert!(err.contains("overlaps"));
    }

    #[test]
    fn split_slice_atomic_contiguous() {
        let (_dir, db) = test_db();
        let t = sample_task(&db, "Split");
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let at = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let slice = db.insert_closed_slice(&t.id, start, end).unwrap();

        let (first, second) = db
            .split_slice(SplitSliceInput {
                id: slice.id.clone(),
                at,
            })
            .unwrap();
        assert_eq!(first.end, Some(at));
        assert_eq!(second.start, at);
        assert_eq!(second.end, Some(end));

        let err = db
            .split_slice(SplitSliceInput {
                id: first.id,
                at: start,
            })
            .unwrap_err();
        assert!(err.contains("inside"));
    }

    #[test]
    fn move_merges_adjacent_same_task() {
        let (_dir, db) = test_db();
        let a = sample_task(&db, "A");
        let b = sample_task(&db, "B");
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let first = db.insert_closed_slice(&a.id, t0, t1).unwrap();
        let second = db.insert_closed_slice(&b.id, t1, t2).unwrap();

        db.update_slice(UpdateSliceInput {
            id: second.id.clone(),
            task_id: Some(a.id.clone()),
            start: None,
            end: None,
        })
        .unwrap();
        let merged = db.merge_adjacent_same_task(&second.id).unwrap();
        assert_eq!(merged.start, t0);
        assert_eq!(merged.end, Some(t2));
        assert_eq!(merged.task_id, a.id);
        assert!(db.get_slice(&first.id).unwrap().is_none() || merged.id == first.id);
        assert_eq!(
            db.list_slices_between(t0, t2 + Duration::seconds(1))
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn merges_same_task_with_sub_minute_gap() {
        let (_dir, db) = test_db();
        let a = sample_task(&db, "A");
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let t2 = t1 + Duration::seconds(30);
        let t3 = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        db.insert_closed_slice(&a.id, t0, t1).unwrap();
        let second = db.insert_closed_slice(&a.id, t2, t3).unwrap();
        db.coalesce_near_adjacent().unwrap();
        let slices = db
            .list_slices_between(t0, t3 + Duration::seconds(1))
            .unwrap();
        assert_eq!(slices.len(), 1);
        assert_eq!(slices[0].start, t0);
        assert_eq!(slices[0].end, Some(t3));
        assert!(db.get_slice(&second.id).unwrap().is_none() || slices[0].id == second.id);
    }

    #[test]
    fn move_does_not_merge_across_gap() {
        let (_dir, db) = test_db();
        let a = sample_task(&db, "A");
        let b = sample_task(&db, "B");
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        let t3 = Utc.with_ymd_and_hms(2026, 1, 1, 13, 0, 0).unwrap();
        db.insert_closed_slice(&a.id, t0, t1).unwrap();
        let second = db.insert_closed_slice(&b.id, t2, t3).unwrap();

        db.update_slice(UpdateSliceInput {
            id: second.id.clone(),
            task_id: Some(a.id.clone()),
            start: None,
            end: None,
        })
        .unwrap();
        let result = db.merge_adjacent_same_task(&second.id).unwrap();
        assert_eq!(result.id, second.id);
        assert_eq!(
            db.list_slices_between(t0, t3 + Duration::seconds(1))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn does_not_merge_when_gap_at_least_one_minute() {
        let (_dir, db) = test_db();
        let a = sample_task(&db, "A");
        let t0 = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 1, 1, 11, 0, 0).unwrap();
        let t2 = t1 + Duration::seconds(60);
        let t3 = Utc.with_ymd_and_hms(2026, 1, 1, 12, 0, 0).unwrap();
        db.insert_closed_slice(&a.id, t0, t1).unwrap();
        db.insert_closed_slice(&a.id, t2, t3).unwrap();
        db.coalesce_near_adjacent().unwrap();
        assert_eq!(
            db.list_slices_between(t0, t3 + Duration::seconds(1))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn running_slice_does_not_block_future_closed_slice() {
        let (_dir, db) = test_db();
        let t = sample_task(&db, "Run");
        let now = Utc::now();
        let run_start = now - Duration::minutes(10);
        let running = db.replace_running_slice(None, &t.id, run_start).unwrap();

        let future_start = now + Duration::minutes(5);
        let future_end = future_start + Duration::minutes(30);
        let created = db
            .insert_closed_slice(&t.id, future_start, future_end)
            .unwrap();
        assert_eq!(created.start, future_start);

        let overlap_start = run_start + Duration::minutes(1);
        let overlap_end = now - Duration::minutes(1);
        let err = db
            .insert_closed_slice(&t.id, overlap_start, overlap_end)
            .unwrap_err();
        assert!(err.contains("overlaps"));

        // Closing past the future slice must clamp so the non-overlap invariant holds.
        db.close_slice(&running.id, future_end + Duration::minutes(10))
            .unwrap();
        let closed = db.get_slice(&running.id).unwrap().unwrap();
        assert_eq!(closed.end, Some(future_start));
    }

    #[test]
    fn update_delete_and_daily_total() {
        let (_dir, db) = test_db();
        let t = sample_task(&db, "Day");
        let now = Utc::now();
        let (day_start, _) = local_day_bounds(now.with_timezone(&chrono::Local).date_naive());
        let closed_start = day_start + Duration::hours(1);
        let closed_end = closed_start + Duration::hours(2);
        let slice = db
            .insert_closed_slice(&t.id, closed_start, closed_end)
            .unwrap();

        let running = db
            .replace_running_slice(None, &t.id, day_start + Duration::hours(4))
            .unwrap();
        let total = db.daily_total_ms(day_start + Duration::hours(5)).unwrap();
        assert_eq!(total, 2 * 3_600_000 + 3_600_000);

        db.close_slice(&running.id, day_start + Duration::hours(5))
            .unwrap();
        db.delete_slice(&slice.id).unwrap();
        assert!(db.get_slice(&slice.id).unwrap().is_none());

        let moved_start = day_start + Duration::hours(6);
        let moved_end = moved_start + Duration::hours(1);
        let other = db
            .insert_closed_slice(&t.id, moved_start, moved_end)
            .unwrap();
        let updated = db
            .update_slice(UpdateSliceInput {
                id: other.id,
                task_id: None,
                start: Some(moved_start + Duration::minutes(15)),
                end: Some(moved_end),
            })
            .unwrap();
        assert_eq!(updated.start, moved_start + Duration::minutes(15));
    }

    #[test]
    fn settings_round_trip() {
        let (_dir, db) = test_db();
        let mut s = db.get_settings().unwrap();
        s.idle_threshold_mins = 15;
        s.daily_goal_mins = 300;
        s.theme = "light".into();
        s.autostart = false;
        s.tracker_always_on_top = false;
        s.tracker_visible = false;
        db.update_settings(&s).unwrap();
        let loaded = db.get_settings().unwrap();
        assert_eq!(loaded.idle_threshold_mins, 15);
        assert_eq!(loaded.daily_goal_mins, 300);
        assert_eq!(loaded.theme, "light");
        assert!(!loaded.autostart);
        assert!(!loaded.tracker_always_on_top);
        assert!(!loaded.tracker_visible);
    }

    #[test]
    fn last_good_at_round_trip() {
        let (_dir, db) = test_db();
        assert!(db.get_last_good_at().unwrap().is_none());
        let at = Utc::now() - Duration::minutes(3);
        db.set_last_good_at(Some(at)).unwrap();
        let loaded = db.get_last_good_at().unwrap().unwrap();
        assert_eq!(loaded.timestamp(), at.timestamp());
        db.set_last_good_at(None).unwrap();
        assert!(db.get_last_good_at().unwrap().is_none());
    }

    #[test]
    fn snooze_until_round_trip() {
        let (_dir, db) = test_db();
        assert!(db.get_snooze_until().unwrap().is_none());
        let until = Utc::now() + Duration::hours(1);
        db.set_snooze_until(Some(until)).unwrap();
        let loaded = db.get_snooze_until().unwrap().unwrap();
        assert_eq!(loaded.timestamp(), until.timestamp());
        db.set_snooze_until(None).unwrap();
        assert!(db.get_snooze_until().unwrap().is_none());
    }

    #[test]
    fn backup_to_and_reopen() {
        let (dir, db) = test_db();
        let t = sample_task(&db, "BackupMe");
        let bak = dir.path().join("export.db");
        db.backup_to(&bak).unwrap();

        let restored = crate::db::open_db(&bak).unwrap();
        let restored_db = Db::new(restored, bak);
        assert_eq!(
            restored_db.get_task(&t.id).unwrap().unwrap().name,
            "BackupMe"
        );

        db.reopen().unwrap();
        assert_eq!(db.get_task(&t.id).unwrap().unwrap().name, "BackupMe");
    }

    #[test]
    fn insert_discard_persists_duration() {
        let (_dir, db) = test_db();
        let start = Utc.with_ymd_and_hms(2026, 1, 1, 10, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2026, 1, 1, 10, 30, 0).unwrap();
        let entry = db.insert_discard(start, end).unwrap();
        assert_eq!(entry.duration_ms, 30 * 60_000);

        let count: i64 = db
            .with_conn(|conn| {
                conn.query_row("SELECT COUNT(*) FROM discard_log", [], |r| r.get(0))
                    .map_err(|e| e.to_string())
            })
            .unwrap();
        assert_eq!(count, 1);
    }
}
