use anyhow::Context;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

use super::{read_auth, CredentialError, StoredAuth};

const TOKEN_URL: &str = "https://auth.openai.com/oauth/token";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";

#[derive(Serialize)]
struct RefreshRequest<'a> {
    client_id: &'a str,
    grant_type: &'a str,
    refresh_token: &'a str,
}

#[derive(Deserialize)]
struct RefreshResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
}

pub(super) async fn refresh(path: &Path, auth: &StoredAuth) -> Result<StoredAuth, CredentialError> {
    let response = reqwest::Client::new()
        .post(TOKEN_URL)
        .json(&RefreshRequest {
            client_id: CLIENT_ID,
            grant_type: "refresh_token",
            refresh_token: &auth.refresh_token,
        })
        .send()
        .await
        .map_err(|error| CredentialError::Temporary(error.into()))?;
    let status = response.status();
    if !status.is_success() {
        let body = response.text().await.unwrap_or_default();
        if status.is_client_error()
            && status != reqwest::StatusCode::REQUEST_TIMEOUT
            && status != reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Err(CredentialError::NeedsSignIn(refresh_error(&body)));
        }
        return Err(CredentialError::Temporary(anyhow::anyhow!(
            "Codex token refresh failed with {status}"
        )));
    }
    let refreshed = response
        .json::<RefreshResponse>()
        .await
        .map_err(|error| CredentialError::Temporary(error.into()))?;
    publish(path, auth, refreshed)
}

fn publish(
    path: &Path,
    used: &StoredAuth,
    refreshed: RefreshResponse,
) -> Result<StoredAuth, CredentialError> {
    let current = read_auth(path).map_err(CredentialError::NeedsSignIn)?;
    if current.refresh_token != used.refresh_token {
        return Ok(current);
    }
    let mut raw = current.raw;
    let tokens = raw
        .pointer_mut("/tokens")
        .and_then(Value::as_object_mut)
        .context("Codex token storage is invalid")
        .map_err(CredentialError::Temporary)?;
    let access_token = refreshed.access_token.ok_or_else(|| {
        CredentialError::Temporary(anyhow::anyhow!("Codex refresh returned no access token"))
    })?;
    tokens.insert("access_token".into(), Value::String(access_token));
    if let Some(refresh_token) = refreshed.refresh_token {
        tokens.insert("refresh_token".into(), Value::String(refresh_token));
    }
    if let Some(root) = raw.as_object_mut() {
        root.insert(
            "last_refresh".into(),
            Value::String(Utc::now().to_rfc3339()),
        );
    }
    let bytes = serde_json::to_vec_pretty(&raw)
        .map_err(|error| CredentialError::Temporary(error.into()))?;
    crate::rotation::write_private_atomic(path, &bytes, "Codex authentication")
        .map_err(CredentialError::Temporary)?;
    read_auth(path).map_err(CredentialError::NeedsSignIn)
}

fn refresh_error(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|value| {
            value
                .pointer("/error/message")
                .or_else(|| value.get("error_description"))
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "Codex sign-in needs to be renewed".to_string())
}
