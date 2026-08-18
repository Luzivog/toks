//! Versioned, sanitized persistence for the last successful limit snapshot.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use super::{LimitSnapshot, Provider, SnapshotFreshness, SnapshotStatus};
use crate::accounts::AccountProfile;

const CACHE_VERSION: u8 = 2;
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

#[derive(Serialize, Deserialize)]
struct CacheEnvelope {
    version: u8,
    snapshot: LimitSnapshot,
}

pub(super) fn load(profile: &AccountProfile) -> Option<LimitSnapshot> {
    let path = cache_file(profile)?;
    let envelope = read_envelope(&path).ok()?;
    if !matches!(envelope.version, 1 | CACHE_VERSION)
        || envelope.snapshot.provider != profile.provider
        || (envelope.version == CACHE_VERSION && envelope.snapshot.account.id != profile.account.id)
    {
        return None;
    }
    let migrate = envelope.version < CACHE_VERSION;
    let mut snapshot = with_profile_identity(envelope.snapshot, profile);
    snapshot.status = SnapshotStatus::at(SnapshotFreshness::Cached);
    snapshot.source = "cache".into();
    snapshot.issue = None;
    if migrate {
        let _ = store(profile, &snapshot);
    }
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
    let path = cache_file(profile).context("no local data directory")?;
    let stored = sanitized_snapshot(profile, snapshot);
    write_envelope(
        &path,
        &CacheEnvelope {
            version: CACHE_VERSION,
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

fn cache_file(profile: &AccountProfile) -> Option<PathBuf> {
    let identity: String = profile
        .account
        .id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.') {
                c
            } else {
                '_'
            }
        })
        .collect();
    dirs::data_local_dir().or_else(dirs::data_dir).map(|root| {
        root.join("tokscope")
            .join("limits")
            .join(format!("{}-{identity}.json", profile.provider.slug()))
    })
}

fn read_envelope(path: &Path) -> Result<CacheEnvelope> {
    let bytes = fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_slice(&bytes).with_context(|| format!("parsing {}", path.display()))
}

fn write_envelope(path: &Path, envelope: &CacheEnvelope) -> Result<()> {
    let parent = path.parent().context("cache path has no parent")?;
    fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    secure_directory(parent)?;
    let temporary = unique_temp_path(path);
    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(&serde_json::to_vec(envelope)?)?;
        file.sync_all()?;
        fs::rename(&temporary, path)?;
        fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
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

#[cfg(test)]
pub(super) fn round_trip_for_test(path: &Path, snapshot: LimitSnapshot) -> Result<LimitSnapshot> {
    write_envelope(
        path,
        &CacheEnvelope {
            version: CACHE_VERSION,
            snapshot,
        },
    )?;
    Ok(read_envelope(path)?.snapshot)
}

#[cfg(test)]
pub(super) fn decode_envelope_for_test(raw: &[u8]) -> Result<(u8, LimitSnapshot)> {
    let envelope: CacheEnvelope = serde_json::from_slice(raw)?;
    Ok((envelope.version, envelope.snapshot))
}
