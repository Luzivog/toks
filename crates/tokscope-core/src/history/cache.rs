//! Crash-safe persistence for the last successful aggregate history snapshot.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use super::HistorySnapshot;

const CACHE_VERSION: u8 = 1;
const MAX_CACHE_BYTES: u64 = 64 * 1024 * 1024;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CacheEnvelope {
    version: u8,
    snapshot: HistorySnapshot,
}

pub(super) fn load() -> Option<HistorySnapshot> {
    load_from(&cache_file()?).ok()
}

pub(super) fn store(snapshot: &HistorySnapshot) -> Result<()> {
    let path = cache_file().context("no local data directory")?;
    store_at(&path, snapshot)
}

/// Preserve the pre-archive aggregate for inspection without treating it as
/// event-level history. Aggregate rows have no identities and cannot be merged
/// safely with the durable archive.
pub(super) fn preserve_legacy_snapshot() {
    let Some(path) = cache_file() else {
        return;
    };
    let Some(parent) = path.parent() else {
        return;
    };
    let legacy = parent.join("snapshot-before-archive.json");
    if legacy.exists() || load_from(&path).is_err() {
        return;
    }
    let temporary = unique_temp_path(&legacy);
    let result = (|| {
        let mut source = fs::File::open(&path)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut target = options.open(&temporary)?;
        std::io::copy(&mut source, &mut target)?;
        target.sync_all()?;
        fs::rename(&temporary, &legacy)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

fn cache_file() -> Option<PathBuf> {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .map(|root| root.join("tokscope/history/snapshot.json"))
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
    super::validation::validate(snapshot)?;
    let parent = path.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent)?;
    secure_directory(parent)?;
    let temporary = unique_temp_path(path);
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        serde_json::to_writer(
            &mut file,
            &CacheEnvelope {
                version: CACHE_VERSION,
                snapshot: snapshot.clone(),
            },
        )?;
        file.flush()?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn unique_temp_path(path: &Path) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_extension(format!("tmp-{}-{nanos:x}-{sequence:x}", std::process::id()))
}

fn secure_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn load_at(path: &Path) -> Option<HistorySnapshot> {
    load_from(path).ok()
}

#[cfg(test)]
pub(super) fn store_at_for_test(path: &Path, snapshot: &HistorySnapshot) -> Result<()> {
    store_at(path, snapshot)
}
