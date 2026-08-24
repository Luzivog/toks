use std::path::PathBuf;

#[cfg(test)]
use serde_json::Value;

use crate::accounts::{AccountId, AccountProfile, CodexAuthSnapshot, CredentialProfileId};
use crate::limits::Provider;

#[cfg(test)]
use super::read_auth;
use super::{Credential, CredentialError, StoredAuth};

pub(super) struct CredentialSnapshot {
    pub(super) account_id: AccountId,
    pub(super) profile_id: CredentialProfileId,
    pub(super) path: PathBuf,
    pub(super) auth: StoredAuth,
    managed: bool,
}

pub(super) fn account_ids() -> Vec<AccountId> {
    profiles()
        .map(|profile| {
            snapshot(&profile)
                .map(|snapshot| snapshot.account_id)
                .unwrap_or(profile.account.id)
        })
        .collect()
}

pub(super) fn snapshots() -> Vec<CredentialSnapshot> {
    let mut snapshots = profiles()
        .filter_map(|profile| snapshot(&profile).ok())
        .collect::<Vec<_>>();
    snapshots.sort_by_key(|snapshot| !snapshot.managed);
    snapshots
}

fn profiles() -> impl Iterator<Item = AccountProfile> {
    crate::accounts::discover_profiles()
        .into_iter()
        .filter(|profile| profile.provider == Provider::Codex)
}

pub(super) fn preferred_snapshot(account: &AccountId) -> Option<CredentialSnapshot> {
    snapshots()
        .into_iter()
        .find(|snapshot| &snapshot.account_id == account)
}

pub(super) fn credential(
    profile: CredentialProfileId,
    expected: &AccountId,
    auth: StoredAuth,
) -> Result<Credential, CredentialError> {
    if !crate::limits::codex::account_header_matches_auth(&auth.raw, &auth.chatgpt_account_id) {
        return Err(CredentialError::NeedsSignIn(
            "Codex credential account fields disagree".into(),
        ));
    }
    let observed =
        crate::accounts::codex_auth_account_id(&profile, &auth.raw).ok_or_else(|| {
            CredentialError::NeedsSignIn("Codex credential identity is unverifiable".into())
        })?;
    if &observed != expected {
        return Err(CredentialError::NeedsSignIn(
            "Codex credential identity changed while it was being read".into(),
        ));
    }
    Ok(Credential {
        account_id: observed,
        access_token: auth.access_token,
        chatgpt_account_id: auth.chatgpt_account_id,
    })
}

fn snapshot(profile: &AccountProfile) -> Result<CredentialSnapshot, String> {
    from_auth(profile, CodexAuthSnapshot::read(profile)?)
}

#[cfg(test)]
fn snapshot_with(
    profile: &AccountProfile,
    identify: fn(&CredentialProfileId, &Value) -> Option<AccountId>,
) -> Result<CredentialSnapshot, String> {
    let path = profile.config_dir.join("auth.json");
    let auth = read_auth(&path)?;
    let account_id = identify(&profile.profile_id, &auth.raw)
        .ok_or_else(|| "Codex credential identity is unverifiable".to_string())?;
    from_parts(profile, path, account_id, auth)
}

fn from_auth(
    profile: &AccountProfile,
    auth: CodexAuthSnapshot,
) -> Result<CredentialSnapshot, String> {
    let refresh_token = auth
        .refresh_token
        .ok_or_else(|| "Codex refresh token is missing".to_string())?;
    let chatgpt_account_id = auth
        .chatgpt_account_id
        .ok_or_else(|| "Codex account identity is missing".to_string())?;
    let stored = StoredAuth {
        raw: auth.raw,
        access_token: auth.access_token,
        refresh_token,
        chatgpt_account_id,
    };
    from_parts(profile, auth.path, auth.account_id, stored)
}

fn from_parts(
    profile: &AccountProfile,
    path: PathBuf,
    account_id: AccountId,
    auth: StoredAuth,
) -> Result<CredentialSnapshot, String> {
    Ok(CredentialSnapshot {
        account_id,
        profile_id: profile.profile_id.clone(),
        path,
        auth,
        managed: profile.managed,
    })
}

#[cfg(test)]
mod tests;
