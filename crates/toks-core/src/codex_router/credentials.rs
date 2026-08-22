use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use serde_json::Value;
use std::fs;
use std::path::Path;

use crate::accounts::{AccountId, AccountProfile};
use crate::limits::Provider;

mod refresh;

pub(crate) struct Credential {
    pub account_id: AccountId,
    pub access_token: String,
    pub chatgpt_account_id: String,
}

#[derive(Debug)]
pub(crate) enum CredentialError {
    NeedsSignIn(String),
    Temporary(anyhow::Error),
}

pub(crate) fn account_ids() -> Vec<AccountId> {
    let mut ids = profiles()
        .into_iter()
        .map(|profile| profile.account.id)
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

pub(crate) fn incoming_token_is_enrolled(token: &str) -> bool {
    profiles().iter().any(|profile| {
        read_auth(&profile.config_dir.join("auth.json"))
            .is_ok_and(|auth| auth.access_token == token)
    })
}

pub(crate) async fn for_account(account_id: &AccountId) -> Result<Credential, CredentialError> {
    let profile = preferred_profile(account_id).ok_or_else(|| {
        CredentialError::NeedsSignIn("No Codex credential profile was found".into())
    })?;
    let path = profile.config_dir.join("auth.json");
    let mut auth = read_auth(&path).map_err(CredentialError::NeedsSignIn)?;
    if expires_soon(&auth.access_token) {
        auth = refresh::refresh(&path, &auth).await?;
    }
    Ok(Credential {
        account_id: account_id.clone(),
        access_token: auth.access_token,
        chatgpt_account_id: auth.chatgpt_account_id,
    })
}

pub(crate) async fn refresh_account(account_id: &AccountId) -> Result<Credential, CredentialError> {
    let profile = preferred_profile(account_id).ok_or_else(|| {
        CredentialError::NeedsSignIn("No Codex credential profile was found".into())
    })?;
    let path = profile.config_dir.join("auth.json");
    let auth = read_auth(&path).map_err(CredentialError::NeedsSignIn)?;
    let refreshed = refresh::refresh(&path, &auth).await?;
    Ok(Credential {
        account_id: account_id.clone(),
        access_token: refreshed.access_token,
        chatgpt_account_id: refreshed.chatgpt_account_id,
    })
}

fn profiles() -> Vec<AccountProfile> {
    crate::accounts::discover_profiles()
        .into_iter()
        .filter(|profile| profile.provider == Provider::Codex)
        .collect()
}

fn preferred_profile(account_id: &AccountId) -> Option<AccountProfile> {
    let mut matches = profiles()
        .into_iter()
        .filter(|profile| &profile.account.id == account_id)
        .collect::<Vec<_>>();
    matches.sort_by_key(|profile| !profile.managed);
    matches
        .into_iter()
        .find(|profile| profile.config_dir.join("auth.json").is_file())
}

pub(super) struct StoredAuth {
    pub(super) raw: Value,
    pub(super) access_token: String,
    pub(super) refresh_token: String,
    pub(super) chatgpt_account_id: String,
}

pub(super) fn read_auth(path: &Path) -> Result<StoredAuth, String> {
    let raw = fs::read_to_string(path).map_err(|_| "Codex sign-in is missing".to_string())?;
    let value = serde_json::from_str::<Value>(&raw)
        .map_err(|_| "Codex sign-in data is invalid".to_string())?;
    let string = |pointer: &str| {
        value
            .pointer(pointer)
            .and_then(Value::as_str)
            .filter(|token| !token.is_empty())
            .map(str::to_string)
    };
    Ok(StoredAuth {
        access_token: string("/tokens/access_token")
            .ok_or_else(|| "Codex access token is missing".to_string())?,
        refresh_token: string("/tokens/refresh_token")
            .ok_or_else(|| "Codex refresh token is missing".to_string())?,
        chatgpt_account_id: string("/tokens/account_id")
            .ok_or_else(|| "Codex account identity is missing".to_string())?,
        raw: value,
    })
}

fn expires_soon(token: &str) -> bool {
    token_expiry(token).is_some_and(|expiry| expiry <= Utc::now() + Duration::minutes(5))
}

fn token_expiry(token: &str) -> Option<chrono::DateTime<Utc>> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    let value = serde_json::from_slice::<Value>(&bytes).ok()?;
    chrono::DateTime::from_timestamp(value.get("exp")?.as_i64()?, 0)
}

#[cfg(test)]
mod tests {
    use super::token_expiry;

    #[test]
    fn reads_jwt_expiry_without_trusting_other_claims() {
        let payload = base64::Engine::encode(
            &base64::engine::general_purpose::URL_SAFE_NO_PAD,
            br#"{"exp":1893456000,"private":"ignored"}"#,
        );
        let expiry = token_expiry(&format!("header.{payload}.signature")).unwrap();
        assert_eq!(expiry.timestamp(), 1_893_456_000);
    }
}
