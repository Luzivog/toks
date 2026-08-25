use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::SystemTime;

use rusqlite::{Connection, OpenFlags, OptionalExtension};
use serde::Deserialize;

use crate::rotation::ThreadId;

const DISPLAY_TITLE_CHARS: usize = 80;

/// Read-only lookup for the user-facing titles in a Codex home directory.
#[derive(Clone)]
pub struct ThreadTitleStore {
    codex_home: Option<PathBuf>,
    index_cache: Arc<Mutex<IndexCache>>,
}

impl ThreadTitleStore {
    /// Uses one explicit Codex home directory.
    pub fn new(codex_home: PathBuf) -> Self {
        Self::with_home(Some(codex_home))
    }

    /// Uses `CODEX_HOME`, or the current user's default Codex directory.
    pub fn discover() -> Self {
        Self::with_home(crate::limits::codex::codex_home())
    }

    /// Returns the titles found for the requested thread ids.
    pub fn titles(&self, ids: &[ThreadId]) -> BTreeMap<ThreadId, String> {
        if ids.is_empty() {
            return BTreeMap::new();
        }
        let Some(codex_home) = self.codex_home.as_deref() else {
            return BTreeMap::new();
        };
        let catalogue = database_titles(&codex_home.join("state_5.sqlite"), ids);
        let index = self.session_index(&codex_home.join("session_index.jsonl"));

        ids.iter()
            .filter_map(|id| {
                resolve_title(catalogue.get(id), index.get(id)).map(|title| (id.clone(), title))
            })
            .collect()
    }

    fn with_home(codex_home: Option<PathBuf>) -> Self {
        Self {
            codex_home,
            index_cache: Arc::new(Mutex::new(IndexCache::default())),
        }
    }

    fn session_index(&self, path: &Path) -> Arc<BTreeMap<ThreadId, String>> {
        let Some(before) = fingerprint(path) else {
            *self.cache() = IndexCache::default();
            return Arc::default();
        };
        if let Some(names) = self.cache().get(before) {
            return names;
        }
        let Ok(raw) = fs::read(path) else {
            return Arc::default();
        };
        let names = Arc::new(parse_session_index(&raw));
        if fingerprint(path) == Some(before) {
            *self.cache() = IndexCache {
                fingerprint: Some(before),
                names: Arc::clone(&names),
            };
        }
        names
    }

    fn cache(&self) -> MutexGuard<'_, IndexCache> {
        self.index_cache
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Default)]
struct IndexCache {
    fingerprint: Option<FileFingerprint>,
    names: Arc<BTreeMap<ThreadId, String>>,
}

impl IndexCache {
    fn get(&self, fingerprint: FileFingerprint) -> Option<Arc<BTreeMap<ThreadId, String>>> {
        (self.fingerprint == Some(fingerprint)).then(|| Arc::clone(&self.names))
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct FileFingerprint(SystemTime, u64);

fn fingerprint(path: &Path) -> Option<FileFingerprint> {
    let metadata = fs::metadata(path).ok()?;
    Some(FileFingerprint(metadata.modified().ok()?, metadata.len()))
}

#[derive(Deserialize)]
struct SessionIndexEntry {
    id: String,
    thread_name: String,
}

fn parse_session_index(raw: &[u8]) -> BTreeMap<ThreadId, String> {
    let mut names = BTreeMap::new();
    for line in raw.split(|byte| *byte == b'\n') {
        let Ok(entry) = serde_json::from_slice::<SessionIndexEntry>(line) else {
            continue;
        };
        if let Some(name) = normalize(&entry.thread_name) {
            names.insert(ThreadId::new(entry.id), name);
        }
    }
    names
}

struct DatabaseTitle {
    name: Option<String>,
    fallback: Option<String>,
}

fn database_titles(path: &Path, ids: &[ThreadId]) -> BTreeMap<ThreadId, DatabaseTitle> {
    let Ok(connection) = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) else {
        return BTreeMap::new();
    };
    if connection.busy_timeout(std::time::Duration::ZERO).is_err() {
        return BTreeMap::new();
    }
    let Ok(mut statement) =
        connection.prepare("SELECT name, title, preview FROM threads WHERE id = ?1")
    else {
        return BTreeMap::new();
    };
    let mut titles = BTreeMap::new();
    for id in ids {
        let row = statement
            .query_row([id.as_str()], |row| {
                Ok((
                    row.get::<_, Option<String>>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .optional();
        let Ok(Some((name, title, preview))) = row else {
            continue;
        };
        titles.insert(
            id.clone(),
            DatabaseTitle {
                name: name.as_deref().and_then(normalize),
                fallback: title
                    .as_deref()
                    .and_then(normalize)
                    .or_else(|| preview.as_deref().and_then(normalize)),
            },
        );
    }
    titles
}

fn resolve_title(catalogue: Option<&DatabaseTitle>, indexed: Option<&String>) -> Option<String> {
    catalogue
        .and_then(|title| title.name.clone())
        .or_else(|| indexed.cloned())
        .or_else(|| catalogue?.fallback.as_deref().map(display_label))
}

fn normalize(value: &str) -> Option<String> {
    let mut words = value.split_whitespace();
    let mut normalized = words.next()?.to_owned();
    for word in words {
        normalized.push(' ');
        normalized.push_str(word);
    }
    Some(normalized)
}

fn display_label(value: &str) -> String {
    if value.chars().count() <= DISPLAY_TITLE_CHARS {
        return value.to_owned();
    }
    let mut label: String = value.chars().take(DISPLAY_TITLE_CHARS - 1).collect();
    label.push('…');
    label
}
