use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chrono::{Duration, Utc};
use serde_json::Value;
use std::path::Path;

use crate::accounts::AccountId;

mod refresh;
mod snapshot;
use snapshot::preferred_snapshot;

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
    let mut ids = snapshot::account_ids();
    ids.sort();
    ids.dedup();
    ids
}

pub(crate) fn incoming_token_account(token: &str) -> Option<AccountId> {
    snapshot::snapshots()
        .into_iter()
        .find(|snapshot| snapshot.auth.access_token == token)
        .map(|snapshot| snapshot.account_id)
}

pub(crate) async fn for_account(account_id: &AccountId) -> Result<Credential, CredentialError> {
    let snapshot = preferred_snapshot(account_id).ok_or_else(|| {
        CredentialError::NeedsSignIn("No Codex credential profile was found".into())
    })?;
    let mut auth = snapshot.auth;
    if expires_soon(&auth.access_token) {
        auth = refresh::refresh(&snapshot.path, &auth).await?;
    }
    snapshot::credential(snapshot.profile_id, account_id, auth)
}

pub(crate) async fn refresh_account(account_id: &AccountId) -> Result<Credential, CredentialError> {
    let snapshot = preferred_snapshot(account_id).ok_or_else(|| {
        CredentialError::NeedsSignIn("No Codex credential profile was found".into())
    })?;
    let refreshed = refresh::refresh(&snapshot.path, &snapshot.auth).await?;
    snapshot::credential(snapshot.profile_id, account_id, refreshed)
}

pub(super) struct StoredAuth {
    pub(super) raw: Value,
    pub(super) access_token: String,
    pub(super) refresh_token: String,
    pub(super) chatgpt_account_id: String,
}

pub(super) fn read_auth(path: &Path) -> Result<StoredAuth, String> {
    let raw = std::fs::read(path).map_err(|_| "Codex sign-in is missing".to_string())?;
    let value = serde_json::from_slice::<Value>(&raw)
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
