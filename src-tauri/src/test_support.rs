use crate::db::{open_db, Db};
use std::sync::Arc;

/// Temp-directory-backed SQLite DB for unit/integration tests.
pub fn test_db() -> (tempfile::TempDir, Arc<Db>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("timetrove.db");
    let conn = open_db(&path).expect("open_db");
    (dir, Db::new(conn, path))
}
