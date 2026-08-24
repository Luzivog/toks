use anyhow::{anyhow, Result};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::ProfileMetadata;

pub(super) const PROFILE_VERSION: u8 = 1;

pub(super) fn profiles_root() -> Result<PathBuf> {
    crate::paths::account_metadata_dir()
}

pub(super) fn now_millis() -> Result<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .map_err(|error| anyhow!("system clock is before Unix epoch: {error}"))
}

pub(super) fn now_nanos() -> Result<u128> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .map_err(|error| anyhow!("system clock is before Unix epoch: {error}"))
}

pub(super) fn write_metadata(path: &Path, metadata: &ProfileMetadata) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(metadata)?;
    crate::storage::write_private_atomic(path, &bytes, "account metadata")
}
