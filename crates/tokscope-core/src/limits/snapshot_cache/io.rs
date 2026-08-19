//! Atomic, permission-hardened cache envelope I/O.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::limits::LimitSnapshot;

pub(super) const CACHE_VERSION: u8 = 2;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
pub(super) struct CacheEnvelope {
    pub(super) version: u8,
    pub(super) snapshot: LimitSnapshot,
}

pub(super) fn read_envelope(path: &Path) -> Result<CacheEnvelope> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

#[cfg(test)]
pub(super) fn decode_envelope(raw: &[u8]) -> Result<CacheEnvelope> {
    serde_json::from_slice(raw).context("parsing cache envelope")
}

pub(super) fn write_envelope(path: &Path, envelope: &CacheEnvelope) -> Result<()> {
    let parent = path.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    secure_directory(parent)?;
    let temporary = unique_temp_path(path);
    let result = write_and_replace(path, &temporary, parent, envelope);
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

fn write_and_replace(
    path: &Path,
    temporary: &Path,
    parent: &Path,
    envelope: &CacheEnvelope,
) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(temporary)?;
    file.write_all(&serde_json::to_vec(envelope)?)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    fs::File::open(parent)?.sync_all()?;
    Ok(())
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
