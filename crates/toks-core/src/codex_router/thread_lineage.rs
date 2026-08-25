use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;

use crate::rotation::ThreadId;

/// Known Codex classification and display metadata for one thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadLineage {
    pub kind: ThreadLineageKind,
    pub agent_nickname: Option<String>,
}

/// Codex's stored classification for a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThreadLineageKind {
    TopLevel,
    Subagent { parent: Option<ThreadId> },
}

/// Read-only lookup for thread lineage in a Codex home directory.
#[derive(Clone)]
pub struct ThreadLineageStore {
    database: Option<PathBuf>,
}

impl ThreadLineageStore {
    /// Uses one explicit Codex home directory.
    pub fn new(codex_home: PathBuf) -> Self {
        Self {
            database: Some(codex_home.join("state_5.sqlite")),
        }
    }

    /// Uses `CODEX_HOME`, or the current user's default Codex directory.
    pub fn discover() -> Self {
        Self {
            database: crate::limits::codex::codex_home().map(|home| home.join("state_5.sqlite")),
        }
    }

    /// Returns known lineage for the requested thread ids.
    pub fn lineages(&self, ids: &[ThreadId]) -> BTreeMap<ThreadId, ThreadLineage> {
        let Some(database) = self.database.as_deref() else {
            return BTreeMap::new();
        };
        database_lineages(database, ids)
    }
}

fn database_lineages(path: &Path, ids: &[ThreadId]) -> BTreeMap<ThreadId, ThreadLineage> {
    if ids.is_empty() {
        return BTreeMap::new();
    }
    let Ok(connection) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return BTreeMap::new();
    };
    if connection.busy_timeout(std::time::Duration::ZERO).is_err() {
        return BTreeMap::new();
    }
    let query = if has_spawn_edges(&connection) {
        "SELECT t.thread_source, t.source, t.agent_nickname, e.parent_thread_id \
         FROM threads t LEFT JOIN thread_spawn_edges e ON e.child_thread_id = t.id \
         WHERE t.id = ?1"
    } else {
        "SELECT thread_source, source, agent_nickname, NULL FROM threads WHERE id = ?1"
    };
    let Ok(mut statement) = connection.prepare(query) else {
        return BTreeMap::new();
    };
    ids.iter()
        .filter_map(|id| read_lineage(&mut statement, id).map(|lineage| (id.clone(), lineage)))
        .collect()
}

fn has_spawn_edges(connection: &Connection) -> bool {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master \
             WHERE type = 'table' AND name = 'thread_spawn_edges')",
            [],
            |row| row.get(0),
        )
        .unwrap_or(false)
}

fn read_lineage(statement: &mut rusqlite::Statement<'_>, id: &ThreadId) -> Option<ThreadLineage> {
    let row = statement
        .query_row([id.as_str()], |row| {
            Ok((
                row.get::<_, Option<String>>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, Option<String>>(2)?,
                row.get::<_, Option<String>>(3)?,
            ))
        })
        .optional()
        .ok()??;
    let (thread_source, source, agent_nickname, edge_parent) = row;
    let kind = match thread_source.as_deref() {
        Some("user") => ThreadLineageKind::TopLevel,
        Some("subagent") => ThreadLineageKind::Subagent {
            parent: thread_id(edge_parent).or_else(|| source_parent(source.as_deref())),
        },
        _ => return None,
    };
    Some(ThreadLineage {
        kind,
        agent_nickname: normalized(agent_nickname),
    })
}

fn thread_id(value: Option<String>) -> Option<ThreadId> {
    normalized(value).map(ThreadId::new)
}

fn normalized(value: Option<String>) -> Option<String> {
    let value = value?;
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_owned())
}

#[derive(Deserialize)]
struct Source {
    subagent: Option<SubagentSource>,
}

#[derive(Deserialize)]
struct SubagentSource {
    thread_spawn: Option<ThreadSpawnSource>,
}

#[derive(Deserialize)]
struct ThreadSpawnSource {
    parent_thread_id: String,
}

fn source_parent(source: Option<&str>) -> Option<ThreadId> {
    let source = serde_json::from_str::<Source>(source?).ok()?;
    thread_id(Some(source.subagent?.thread_spawn?.parent_thread_id))
}
