use std::path::PathBuf;

use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::limits::Provider;

use super::{codex_auth_account_id, AccountId, AccountProfile, CredentialProfileId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CodexAuthProof {
    profile_id: CredentialProfileId,
    account_id: AccountId,
    path: PathBuf,
    revision: [u8; 32],
    credential_fingerprint: String,
}

impl CodexAuthProof {
    pub(crate) fn matches_profile(&self, profile: &AccountProfile) -> bool {
        profile.provider == Provider::Codex
            && self.profile_id == profile.profile_id
            && self.account_id == profile.account.id
    }

    pub(crate) fn is_current(&self, profile: &AccountProfile) -> bool {
        self.matches_profile(profile)
            && self.path == profile.config_dir.join("auth.json")
            && self.auth_file_is_current()
    }

    pub(crate) fn account_id(&self) -> &AccountId {
        &self.account_id
    }

    pub(crate) fn profile_id(&self) -> &CredentialProfileId {
        &self.profile_id
    }

    pub(crate) fn auth_file_is_current(&self) -> bool {
        std::fs::read(&self.path)
            .ok()
            .is_some_and(|raw| revision(&raw) == self.revision)
    }

    pub(crate) fn credential_fingerprint(&self) -> &str {
        &self.credential_fingerprint
    }

    pub(crate) fn revision(&self) -> [u8; 32] {
        self.revision
    }
}

pub(crate) struct CodexAuthSnapshot {
    pub(crate) account_id: AccountId,
    pub(crate) profile_id: CredentialProfileId,
    pub(crate) path: PathBuf,
    pub(crate) raw: Value,
    pub(crate) access_token: String,
    pub(crate) refresh_token: Option<String>,
    pub(crate) chatgpt_account_id: Option<String>,
    revision: [u8; 32],
}

impl CodexAuthSnapshot {
    pub(crate) fn read(profile: &AccountProfile) -> Result<Self, String> {
        read_with(profile, codex_auth_account_id)
    }

    pub(crate) fn proof(&self) -> CodexAuthProof {
        let chatgpt_account_id = self
            .chatgpt_account_id
            .as_deref()
            .expect("validated Codex auth has an account ID");
        CodexAuthProof {
            profile_id: self.profile_id.clone(),
            account_id: self.account_id.clone(),
            path: self.path.clone(),
            revision: self.revision,
            credential_fingerprint: credential_fingerprint(&self.access_token, chatgpt_account_id),
        }
    }
}

fn read_with(
    profile: &AccountProfile,
    identify: fn(&CredentialProfileId, &Value) -> Option<AccountId>,
) -> Result<CodexAuthSnapshot, String> {
    if profile.provider != Provider::Codex {
        return Err("credential profile is not a Codex profile".into());
    }
    let path = profile.config_dir.join("auth.json");
    let bytes = std::fs::read(&path).map_err(|_| "Codex sign-in is missing".to_string())?;
    let raw = serde_json::from_slice::<Value>(&bytes)
        .map_err(|_| "Codex sign-in data is invalid".to_string())?;
    let string = |pointer: &str| {
        raw.pointer(pointer)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
    };
    let access_token = string("/tokens/access_token")
        .ok_or_else(|| "Codex access token is missing".to_string())?;
    let chatgpt_account_id = string("/tokens/account_id")
        .ok_or_else(|| "Codex account identity is missing".to_string())?;
    if !crate::limits::codex::account_header_matches_auth(&raw, &chatgpt_account_id) {
        return Err("Codex credential account fields disagree".into());
    }
    let account_id = identify(&profile.profile_id, &raw)
        .ok_or_else(|| "Codex credential identity is unverifiable".to_string())?;
    Ok(CodexAuthSnapshot {
        account_id,
        profile_id: profile.profile_id.clone(),
        path,
        refresh_token: string("/tokens/refresh_token"),
        chatgpt_account_id: Some(chatgpt_account_id),
        access_token,
        revision: revision(&bytes),
        raw,
    })
}

fn revision(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

pub(crate) fn credential_fingerprint(access_token: &str, chatgpt_account_id: &str) -> String {
    let mut digest = Sha256::new();
    digest.update(b"toks.codex.credential.v1\0");
    digest.update(
        u64::try_from(access_token.len())
            .expect("credential token length is bounded")
            .to_be_bytes(),
    );
    digest.update(access_token.as_bytes());
    digest.update(
        u64::try_from(chatgpt_account_id.len())
            .expect("credential account ID length is bounded")
            .to_be_bytes(),
    );
    digest.update(chatgpt_account_id.as_bytes());
    format!("sha256:{:x}", digest.finalize())
}

#[cfg(test)]
pub(crate) fn read_for_test(profile: &AccountProfile) -> Result<CodexAuthSnapshot, String> {
    read_with(profile, super::codex_auth_account_id_for_test)
}
