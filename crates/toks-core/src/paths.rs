//! Canonical paths for Toks-owned durable data.

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::accounts::CredentialProfileId;
use crate::Provider;

pub(crate) fn data_dir() -> Result<PathBuf> {
    toks_ingest::paths::get_data_dir().context("no local data directory")
}

pub(crate) fn account_order_file() -> Result<PathBuf> {
    Ok(data_dir()?.join("account-order.json"))
}

pub(crate) fn account_metadata_dir() -> Result<PathBuf> {
    Ok(data_dir()?.join("profiles"))
}

pub(crate) fn account_identity_key() -> Result<PathBuf> {
    Ok(data_dir()?.join("account-principal.key"))
}

pub(crate) fn account_suppression_store() -> Result<PathBuf> {
    Ok(data_dir()?.join("account-suppression.json"))
}

pub(crate) fn account_activation_store() -> Result<PathBuf> {
    Ok(data_dir()?.join("rotation/account-activation.json"))
}

pub(crate) fn codex_config_backup() -> Result<PathBuf> {
    Ok(data_dir()?.join("rotation/codex-config-backup.json"))
}

pub(crate) fn router_deployment_state() -> Result<PathBuf> {
    Ok(router_deployment_state_at(&data_dir()?))
}

pub(crate) fn router_deployment_state_at(data: &Path) -> PathBuf {
    data.join("rotation/router-host.json")
}

pub(crate) fn router_artifacts_dir() -> Result<PathBuf> {
    Ok(router_artifacts_dir_at(&data_dir()?))
}

pub(crate) fn router_artifacts_dir_at(data: &Path) -> PathBuf {
    data.join("rotation/router-artifacts")
}

pub(crate) fn proxy_inbound_store() -> Result<PathBuf> {
    Ok(data_dir()?.join("rotation/inbound-tokens.json"))
}

pub(crate) fn resume_state_store_at(data: &Path) -> PathBuf {
    data.join("rotation/resume-state.json")
}

pub(crate) fn resume_outcomes_dir_at(data: &Path) -> PathBuf {
    data.join("rotation/resume-outcomes")
}

pub(crate) fn resume_supervisor_lock_at(data: &Path) -> PathBuf {
    data.join("rotation/resume-supervisor.lock")
}

pub(crate) fn remote_control_store() -> Result<PathBuf> {
    Ok(data_dir()?.join("remote-control.json"))
}

pub(crate) fn limits_snapshot_cache(
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> Result<PathBuf> {
    Ok(limits_snapshot_cache_at(&data_dir()?, provider, profile_id))
}

pub(crate) fn limits_snapshot_cache_at(
    data: &Path,
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> PathBuf {
    let identity: String = profile_id
        .as_str()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    data.join("limits")
        .join(format!("{}-{identity}.json", provider.slug()))
}

pub(crate) fn history_cache() -> Result<PathBuf> {
    Ok(data_dir()?.join("history/snapshot.json"))
}

pub(crate) fn history_legacy_cache() -> Result<PathBuf> {
    Ok(data_dir()?.join("history/snapshot-before-archive.json"))
}

pub(crate) fn history_archive() -> Result<PathBuf> {
    Ok(data_dir()?.join("history/usage.sqlite3"))
}

pub(crate) fn rotation_settings() -> Result<PathBuf> {
    Ok(rotation_settings_at(&data_dir()?))
}

pub(crate) fn rotation_settings_at(data: &Path) -> PathBuf {
    data.join("rotation/settings.json")
}

pub(crate) fn rotation_runtime() -> Result<PathBuf> {
    Ok(rotation_runtime_at(&data_dir()?))
}

pub(crate) fn rotation_runtime_at(data: &Path) -> PathBuf {
    data.join("rotation/runtime.json")
}
