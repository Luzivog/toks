//! Versioned, sanitized persistence for the last successful limit snapshot.

mod io;
mod storage;

#[cfg(test)]
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{LimitSnapshot, Provider, SnapshotFreshness, SnapshotStatus};
use crate::accounts::{AccountProfile, CredentialProfileId};
use io::CacheEnvelope;

pub(super) fn load(profile: &AccountProfile) -> Option<LimitSnapshot> {
    let path = storage::cache_file(profile).ok()?;
    let envelope = io::read_envelope(&path).ok()?;
    if !matches_profile(&envelope, profile) {
        return None;
    }
    let mut snapshot = with_profile_identity(envelope.snapshot, profile);
    snapshot.status = SnapshotStatus::at(SnapshotFreshness::Cached);
    snapshot.source = "cache".into();
    snapshot.issue = None;
    Some(snapshot)
}

pub(super) fn load_or_seed(profile: &AccountProfile) -> Option<LimitSnapshot> {
    load(profile).or_else(|| {
        let snapshot = match profile.provider {
            Provider::Claude => {
                super::claude::read_from_profile(&profile.home_dir, &profile.config_dir).ok()
            }
            Provider::Codex => super::codex::read_from_home(&profile.config_dir).ok(),
        }?;
        let mut snapshot = with_profile_identity(snapshot, profile);
        snapshot.status = SnapshotStatus::at(SnapshotFreshness::ProviderCache);
        snapshot.source = "provider_cache".into();
        snapshot.issue = None;
        let _ = store(profile, &snapshot);
        Some(snapshot)
    })
}

pub(super) fn store(profile: &AccountProfile, snapshot: &LimitSnapshot) -> Result<()> {
    if !storage::profile_storage_active(profile) {
        anyhow::bail!("account profile was removed while usage was refreshing");
    }
    let path = storage::cache_file(profile)?;
    let stored = sanitized_snapshot(profile, snapshot);
    io::write_envelope(
        &path,
        &CacheEnvelope {
            version: io::CACHE_VERSION,
            snapshot: stored,
        },
    )
}

pub(super) fn sanitized_snapshot(
    profile: &AccountProfile,
    snapshot: &LimitSnapshot,
) -> LimitSnapshot {
    let mut stored = with_profile_identity(snapshot.clone(), profile);
    stored.account.email = None;
    stored.issue = None;
    stored.status.issue = None;
    stored.source = match stored.status.freshness {
        SnapshotFreshness::Live => "live",
        SnapshotFreshness::ProviderCache => "provider_cache",
        _ => "cache",
    }
    .into();
    stored.extras.clear();
    for window in &mut stored.windows {
        window.raw = serde_json::Value::Null;
    }
    stored
}

fn with_profile_identity(mut snapshot: LimitSnapshot, profile: &AccountProfile) -> LimitSnapshot {
    let email = profile
        .account
        .email
        .clone()
        .or(snapshot.account.email.take());
    snapshot.account = profile.account.clone();
    snapshot.account.email = email;
    snapshot
}

fn matches_profile(envelope: &CacheEnvelope, profile: &AccountProfile) -> bool {
    envelope.version == io::CACHE_VERSION
        && envelope.snapshot.provider == profile.provider
        && envelope.snapshot.account.id == profile.account.id
}

pub(crate) fn remove_for_profile(
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> Result<()> {
    storage::remove_for_profile(provider, profile_id)
}

#[cfg(test)]
pub(super) fn round_trip_for_test(path: &Path, snapshot: LimitSnapshot) -> Result<LimitSnapshot> {
    io::write_envelope(
        path,
        &CacheEnvelope {
            version: io::CACHE_VERSION,
            snapshot,
        },
    )?;
    Ok(io::read_envelope(path)?.snapshot)
}

#[cfg(test)]
pub(super) fn decode_envelope_for_test(raw: &[u8]) -> Result<(u8, LimitSnapshot)> {
    let envelope = io::decode_envelope(raw)?;
    Ok((envelope.version, envelope.snapshot))
}

#[cfg(test)]
pub(super) fn cache_binding_for_test(
    root: &Path,
    profile: &AccountProfile,
    snapshot: LimitSnapshot,
) -> (PathBuf, bool) {
    let envelope = CacheEnvelope {
        version: io::CACHE_VERSION,
        snapshot,
    };
    (
        storage::cache_file_in(root, profile.provider, &profile.profile_id),
        matches_profile(&envelope, profile),
    )
}

#[cfg(test)]
pub(super) fn profile_storage_active_for_test(profile: &AccountProfile) -> bool {
    storage::profile_storage_active(profile)
}
