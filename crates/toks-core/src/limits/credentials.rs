use std::fs;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::http::LiveError;
use super::{LimitIssueKind, Provider};
use crate::accounts::AccountProfile;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CredentialRevision {
    digest: [u8; 32],
}

impl CredentialRevision {
    pub(crate) fn from_digest(digest: [u8; 32]) -> Self {
        Self { digest }
    }
}

pub(crate) fn present(profile: &AccountProfile) -> bool {
    path(profile).is_file()
}

/// A token rotation must invalidate live backoff immediately, including an
/// atomic same-size replacement whose filesystem timestamp is unchanged.
pub(crate) fn revision(profile: &AccountProfile) -> Option<CredentialRevision> {
    let bytes = fs::read(path(profile)).ok()?;
    Some(CredentialRevision::from_digest(
        Sha256::digest(bytes).into(),
    ))
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

pub(crate) fn storage_error(error: impl std::fmt::Display) -> LiveError {
    LiveError::new(LimitIssueKind::Storage, error.to_string())
}
