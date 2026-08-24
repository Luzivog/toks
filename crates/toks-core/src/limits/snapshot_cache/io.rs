//! Atomic, permission-hardened cache envelope I/O.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::limits::LimitSnapshot;

pub(super) const CACHE_VERSION: u8 = 2;

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
    crate::storage::restrict_directory(parent)?;
    let bytes = serde_json::to_vec(envelope)?;
    crate::storage::write_private_atomic(path, &bytes, "limit snapshot cache")
}
