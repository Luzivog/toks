//! Cache path ownership and profile-lifecycle checks.

use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::accounts::{AccountProfile, CredentialProfileId};
use crate::Provider;

pub(super) fn cache_file(profile: &AccountProfile) -> Result<PathBuf> {
    cache_file_for(profile.provider, &profile.profile_id)
}

#[cfg(test)]
pub(super) fn cache_file_in(
    root: &Path,
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> PathBuf {
    crate::paths::limits_snapshot_cache_at(&root.join("toks"), provider, profile_id)
}

pub(super) fn remove_for_profile(
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> Result<()> {
    let Ok(path) = cache_file_for(provider, profile_id) else {
        return Ok(());
    };
    match fs::remove_file(&path) {
        Ok(()) => {
            if let Some(parent) = path.parent() {
                fs::File::open(parent)?.sync_all()?;
            }
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error).with_context(|| format!("removing {}", path.display())),
    }
}

pub(super) fn profile_storage_active(profile: &AccountProfile) -> bool {
    if !profile.managed {
        return true;
    }
    profile
        .home_dir
        .parent()
        .is_some_and(|root| root.join("profile.json").is_file())
}

fn cache_file_for(provider: Provider, profile_id: &CredentialProfileId) -> Result<PathBuf> {
    crate::paths::limits_snapshot_cache(provider, profile_id)
}
