use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::UNIX_EPOCH;

use serde_json::Value;

use super::http::LiveError;
use super::{LimitIssueKind, Provider};
use crate::accounts::AccountProfile;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CredentialRevision {
    modified_ns: u128,
    length: u64,
}

pub(crate) fn present(profile: &AccountProfile) -> bool {
    path(profile).is_file()
}

/// A token rotation must invalidate live backoff immediately. Metadata is
/// enough for change detection and avoids retaining or hashing credentials.
pub(crate) fn revision(profile: &AccountProfile) -> Option<CredentialRevision> {
    let metadata = fs::metadata(path(profile)).ok()?;
    let modified_ns = metadata
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_nanos();
    Some(CredentialRevision {
        modified_ns,
        length: metadata.len(),
    })
}

fn path(profile: &AccountProfile) -> PathBuf {
    credentials_path(profile.provider, &profile.config_dir)
}

fn credentials_path(provider: Provider, config_dir: &Path) -> PathBuf {
    match provider {
        Provider::Claude => config_dir.join(".credentials.json"),
        Provider::Codex => config_dir.join("auth.json"),
    }
}

pub(crate) fn write_json_atomically(path: &Path, value: &Value) -> Result<(), LiveError> {
    let parent = path
        .parent()
        .ok_or_else(|| LiveError::new(LimitIssueKind::Storage, "credential path has no parent"))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = path.with_extension(format!("tmp-{}-{sequence:x}", std::process::id()));
    let result = (|| -> std::io::Result<()> {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(value)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result.map_err(|error| LiveError::new(LimitIssueKind::Storage, error.to_string()))
}
