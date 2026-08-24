//! Crash-safe persistence for the last successful aggregate history snapshot.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::HistorySnapshot;

const CACHE_VERSION: u8 = 1;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEnvelope {
    version: u8,
    snapshot: HistorySnapshot,
}

#[derive(Serialize)]
struct CacheEnvelopeRef<'a> {
    version: u8,
    snapshot: &'a HistorySnapshot,
}

pub(super) fn load() -> Option<HistorySnapshot> {
    load_from(&cache_file().ok()?).ok()
}

pub(super) fn store(snapshot: &HistorySnapshot) -> Result<()> {
    let path = cache_file()?;
    store_at(&path, snapshot)
}

/// Preserve the pre-archive aggregate for inspection without treating it as
/// event-level history. Aggregate rows have no identities and cannot be merged
/// safely with the durable archive.
pub(super) fn preserve_legacy_snapshot() {
    let Some(path) = cache_file().ok() else {
        return;
    };
    let Some(legacy) = crate::paths::history_legacy_cache().ok() else {
        return;
    };
    if legacy.exists() || load_from(&path).is_err() {
        return;
    }
    if let Ok(bytes) = fs::read(path) {
        let _ = crate::storage::write_private_atomic(&legacy, &bytes, "legacy history snapshot");
    }
}

fn cache_file() -> Result<PathBuf> {
    crate::paths::history_cache()
}

fn load_from(path: &Path) -> Result<HistorySnapshot> {
    let metadata = fs::metadata(path)?;
    if metadata.len() > MAX_CACHE_BYTES {
        bail!("history snapshot exceeds size limit");
    }
    let bytes = fs::read(path)?;
    let envelope: CacheEnvelope = serde_json::from_slice(&bytes)?;
    if envelope.version != CACHE_VERSION {
        bail!("unsupported history snapshot version");
    }
    super::validation::validate(&envelope.snapshot)?;
    Ok(envelope.snapshot)
}

fn store_at(path: &Path, snapshot: &HistorySnapshot) -> Result<()> {
    store_at_with_limit(path, snapshot, MAX_CACHE_BYTES)
}

fn store_at_with_limit(path: &Path, snapshot: &HistorySnapshot, limit: u64) -> Result<()> {
    super::validation::validate(snapshot)?;
    let parent = path.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent)?;
    crate::storage::restrict_directory(parent)?;
    crate::storage::write_private_atomic_capped(path, limit, "history snapshot", |writer| {
        serde_json::to_writer(
            writer,
            &CacheEnvelopeRef {
                version: CACHE_VERSION,
                snapshot,
            },
        )?;
        Ok(())
    })
}

#[cfg(test)]
pub(super) fn load_at(path: &Path) -> Option<HistorySnapshot> {
    load_from(path).ok()
}

#[cfg(test)]
pub(super) fn store_at_for_test(path: &Path, snapshot: &HistorySnapshot) -> Result<()> {
    store_at(path, snapshot)
}

#[cfg(test)]
pub(super) fn store_at_with_limit_for_test(
    path: &Path,
    snapshot: &HistorySnapshot,
    limit: u64,
) -> Result<()> {
    store_at_with_limit(path, snapshot, limit)
}
