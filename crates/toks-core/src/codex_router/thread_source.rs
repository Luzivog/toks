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
        let codex_home = crate::limits::codex::codex_home();
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
