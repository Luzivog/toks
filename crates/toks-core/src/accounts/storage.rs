use anyhow::{anyhow, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::ProfileMetadata;

pub(super) const PROFILE_VERSION: u8 = 1;

pub(super) fn profiles_root() -> Result<PathBuf> {
    toks_ingest::paths::get_data_dir()
        .map(|dir| dir.join("profiles"))
        .context("no local data directory")
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
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).context("creating account metadata")?;
    file.write_all(&bytes).context("writing account metadata")
}

pub(super) fn restrict_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .context("securing account profile directory")?;
    }
    Ok(())
}
