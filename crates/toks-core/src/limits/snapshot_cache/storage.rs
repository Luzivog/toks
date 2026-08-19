//! Cache path ownership and profile-lifecycle checks.

use std::fs;
#[cfg(test)]
use std::path::Path;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::accounts::{AccountProfile, CredentialProfileId};
use crate::Provider;

pub(super) fn cache_file(profile: &AccountProfile) -> Option<PathBuf> {
    cache_file_for(profile.provider, &profile.profile_id)
}

#[cfg(test)]
pub(super) fn cache_file_in(
    root: &Path,
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> PathBuf {
    let identity = safe_identity(profile_id);
    root.join("toks")
        .join("limits")
        .join(format!("{}-{identity}.json", provider.slug()))
}

pub(super) fn remove_for_profile(
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> Result<()> {
    let Some(path) = cache_file_for(provider, profile_id) else {
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

fn cache_file_for(provider: Provider, profile_id: &CredentialProfileId) -> Option<PathBuf> {
    toks_ingest::paths::get_data_dir().map(|root| {
        let identity = safe_identity(profile_id);
        root.join("limits")
            .join(format!("{}-{identity}.json", provider.slug()))
    })
}

fn safe_identity(profile_id: &CredentialProfileId) -> String {
    profile_id
        .as_str()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect()
}
