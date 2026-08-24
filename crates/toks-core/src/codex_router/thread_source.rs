use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};

use crate::rotation::ThreadId;

/// Read-only view of Codex's authoritative thread catalogue.
///
/// A missing, older, or temporarily unreadable catalogue is treated as
/// unknown. Only a positive `thread_source = 'subagent'` match suppresses an
/// external resume, so older Codex installations keep their prior behavior.
#[derive(Clone)]
pub(in crate::codex_router) struct ThreadSourceStore {
    database: Option<PathBuf>,
}

impl ThreadSourceStore {
    pub(in crate::codex_router) fn discover() -> Self {
        let codex_home = std::env::var_os("CODEX_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|home| home.join(".codex")));
        Self {
            database: codex_home.map(|home| home.join("state_5.sqlite")),
        }
    }

    pub(in crate::codex_router) fn is_known_subagent(&self, thread: &ThreadId) -> bool {
        self.database
            .as_deref()
            .is_some_and(|database| is_known_subagent(database, thread))
    }

    #[cfg(test)]
    pub(in crate::codex_router) fn for_database(database: impl Into<PathBuf>) -> Self {
        Self {
            database: Some(database.into()),
        }
    }

    #[cfg(test)]
    pub(in crate::codex_router) fn unavailable() -> Self {
        Self { database: None }
    }
}

fn is_known_subagent(database: &Path, thread: &ThreadId) -> bool {
    let Ok(connection) = Connection::open_with_flags(
        database,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return false;
    };
    connection
        .query_row(
            "SELECT EXISTS(\
                SELECT 1 FROM threads \
                WHERE id = ?1 AND thread_source = 'subagent'\
            )",
            [thread.as_str()],
            |row| row.get::<_, bool>(0),
        )
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use rusqlite::Connection;

    use super::ThreadSourceStore;
    use crate::rotation::ThreadId;

    #[test]
    fn only_an_authoritative_subagent_row_blocks_external_resume() {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join("state.sqlite");
        let connection = Connection::open(&database).unwrap();
        connection
            .execute(
                "CREATE TABLE threads (id TEXT PRIMARY KEY, thread_source TEXT)",
                [],
            )
            .unwrap();
        connection
            .execute(
                "INSERT INTO threads (id, thread_source) VALUES \
                    ('child', 'subagent'), ('root', 'cli')",
                [],
            )
            .unwrap();
        drop(connection);
        let sources = ThreadSourceStore::for_database(database);

        assert!(sources.is_known_subagent(&ThreadId::new("child")));
        assert!(!sources.is_known_subagent(&ThreadId::new("root")));
        assert!(!sources.is_known_subagent(&ThreadId::new("unknown")));
    }

    #[test]
    fn missing_or_older_catalogues_fail_open() {
        let directory = tempfile::tempdir().unwrap();
        let missing = ThreadSourceStore::for_database(directory.path().join("missing.sqlite"));
        assert!(!missing.is_known_subagent(&ThreadId::new("child")));

        let database = directory.path().join("old.sqlite");
        let connection = Connection::open(&database).unwrap();
        connection
            .execute("CREATE TABLE threads (id TEXT PRIMARY KEY)", [])
            .unwrap();
        connection
            .execute("INSERT INTO threads (id) VALUES ('child')", [])
            .unwrap();
        drop(connection);
        let older = ThreadSourceStore::for_database(database);
        assert!(!older.is_known_subagent(&ThreadId::new("child")));
        assert!(!ThreadSourceStore::unavailable().is_known_subagent(&ThreadId::new("child")));
    }
}
