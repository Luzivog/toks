use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use crate::Provider;

use super::paths::{reject_symlinks, LifecyclePaths};
use super::types::ManagedRemovalState;
use crate::accounts::{ProfileMetadata, PROFILE_VERSION};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Tombstone {
    version: u8,
    provider: Provider,
    local_profile_id: String,
    history_retained: bool,
}

pub(super) fn remove_managed(
    paths: &LifecyclePaths,
    provider: Provider,
    id: &str,
) -> Result<ManagedRemovalState> {
    let source = paths.profiles_root.join(provider.slug()).join(id);
    let quarantine = paths.quarantine(provider, id);
    let tombstone = paths.tombstone(provider, id);
    let reserved = marker_matches(&source, provider, id);
    let removal_recorded = marker_matches(&tombstone, provider, id);
    if removal_recorded && reserved && !quarantine.exists() {
        return Ok(ManagedRemovalState::AlreadyRemoved);
    }
    if source.exists() && !reserved {
        validate_tree(&source, provider, id, &paths.profiles_root)?;
        let parent = quarantine.parent().context("quarantine has no parent")?;
        fs::create_dir_all(parent).context("creating account-removal quarantine")?;
        crate::storage::restrict_directory(parent)?;
        if quarantine.exists() {
            bail!("account-removal quarantine already contains this profile")
        }
        fs::rename(&source, &quarantine).context("quarantining managed account profile")?;
        // Keep the provider's old absolute config path blocked. A detached CLI
        // child cannot recreate credentials beneath a regular file.
        write_marker(&source, provider, id)?;
        sync_directory(source.parent().context("profile has no parent")?)?;
        sync_directory(parent)?;
    } else if !quarantine.exists() {
        bail!("local account profile was not discovered")
    }
    if !marker_matches(&source, provider, id) {
        write_marker(&source, provider, id)?;
        sync_directory(source.parent().context("profile has no parent")?)?;
    }
    if removal_recorded {
        validate_quarantine_root(&quarantine, &paths.profiles_root)?;
    } else {
        validate_tree(&quarantine, provider, id, &paths.profiles_root)?;
    }
    write_marker(&tombstone, provider, id)?;
    match fs::remove_dir_all(&quarantine) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("deleting quarantined account profile"),
    }
    sync_directory(quarantine.parent().context("quarantine has no parent")?)?;
    Ok(ManagedRemovalState::Removed)
}

fn validate_tree(path: &Path, provider: Provider, id: &str, root: &Path) -> Result<()> {
    reject_symlinks(path)?;
    let canonical_root = fs::canonicalize(root).context("resolving managed profiles root")?;
    let canonical_path = fs::canonicalize(path).context("resolving managed account profile")?;
    if !canonical_path.starts_with(&canonical_root) {
        bail!("managed account profile escaped the Toks profiles root")
    }
    let metadata_path = path.join("profile.json");
    let raw = fs::read(&metadata_path).context("reading managed account metadata")?;
    let metadata: ProfileMetadata =
        serde_json::from_slice(&raw).context("parsing managed account metadata")?;
    if metadata.version != PROFILE_VERSION || metadata.provider != provider || metadata.id != id {
        bail!("managed account metadata does not match the removal target")
    }
    Ok(())
}

fn write_marker(path: &Path, provider: Provider, id: &str) -> Result<()> {
    if path.exists() {
        if marker_matches(path, provider, id) {
            return Ok(());
        }
        bail!("account-removal marker conflicts with an existing path")
    }
    let parent = path.parent().context("removal tombstone has no parent")?;
    fs::create_dir_all(parent).context("creating removal tombstone directory")?;
    crate::storage::restrict_directory(parent)?;
    let bytes = serde_json::to_vec(&Tombstone {
        version: 1,
        provider,
        local_profile_id: id.to_owned(),
        history_retained: true,
    })?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
    let mut file = options.open(path).context("creating removal tombstone")?;
    file.write_all(&bytes)
        .context("writing removal tombstone")?;
    file.sync_all().context("syncing removal tombstone")?;
    sync_directory(parent)
}

fn marker_matches(path: &Path, provider: Provider, id: &str) -> bool {
    let Ok(metadata) = fs::symlink_metadata(path) else {
        return false;
    };
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    fs::read(path)
        .ok()
        .and_then(|raw| serde_json::from_slice::<Tombstone>(&raw).ok())
        .is_some_and(|marker| {
            marker.version == 1
                && marker.provider == provider
                && marker.local_profile_id == id
                && marker.history_retained
        })
}

fn validate_quarantine_root(path: &Path, profiles_root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("inspecting account quarantine")?;
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        bail!("account quarantine is not a private directory")
    }
    let root = fs::canonicalize(profiles_root).context("resolving managed profiles root")?;
    let quarantine = fs::canonicalize(path).context("resolving account quarantine")?;
    if !quarantine.starts_with(root) {
        bail!("account quarantine escaped the Toks profiles root")
    }
    Ok(())
}

fn sync_directory(path: &Path) -> Result<()> {
    fs::File::open(path)
        .and_then(|directory| directory.sync_all())
        .context("syncing account lifecycle directory")
}
