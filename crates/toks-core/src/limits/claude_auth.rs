use std::time::{SystemTime, UNIX_EPOCH};

use reqwest::StatusCode;
use serde_json::{json, Value};

use super::http::{get_json, LiveError};
use super::{LimitIssueKind, Provider};
use crate::accounts::AccountProfile;

const TOKEN_URL: &str = "https://platform.claude.com/v1/oauth/token";
const CLIENT_ID: &str = "9d1c250a-e61b-44d9-88ed-5944d1962f5e";
const REFRESH_SKEW_MS: u128 = 60_000;

pub(crate) fn access_token(profile: &AccountProfile) -> Result<String, LiveError> {
    access_token_at(profile, TOKEN_URL, now_millis(), false, None)
}

pub(crate) fn refresh_after_rejection(
    profile: &AccountProfile,
    rejected_access: &str,
) -> Result<String, LiveError> {
    access_token_at(
        profile,
        TOKEN_URL,
        now_millis(),
        true,
        Some(rejected_access),
    )
}

pub(super) fn access_token_at(
    profile: &AccountProfile,
    endpoint: &str,
    now_ms: u128,
    force: bool,
    rejected_access: Option<&str>,
) -> Result<String, LiveError> {
    debug_assert_eq!(profile.provider, Provider::Claude);
    let path = profile.config_dir.join(".credentials.json");
    let initial = super::claude_credentials::read(&path)?;
    if usable_without_refresh(&initial, now_ms, force, rejected_access) {
        return Ok(initial
            .access
            .expect("usable credentials have an access token"));
    }
    let _lock = super::claude_lock::ClaudeRefreshLock::acquire(&profile.config_dir)?;
    let before = super::claude_credentials::read(&path)?;
    if usable_without_refresh(&before, now_ms, force, rejected_access) {
        return Ok(before
            .access
            .expect("usable credentials have an access token"));
    }
    let refresh = before
        .refresh
        .as_deref()
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            LiveError::new(
                LimitIssueKind::Authentication,
                "Claude sign-in needs to be renewed",
            )
        })?;
    let response = refresh_request(endpoint, refresh, &before.scopes)?;
    let access = response
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .ok_or_else(|| {
            LiveError::new(
                LimitIssueKind::InvalidResponse,
                "Claude refresh returned no access token",
            )
        })?;
    let expires_in = response
        .get("expires_in")
        .and_then(Value::as_u64)
        .filter(|seconds| *seconds > 0)
        .ok_or_else(|| {
            LiveError::new(
                LimitIssueKind::InvalidResponse,
                "Claude refresh returned no expiry",
            )
        })?;

    let current_disk = super::claude_credentials::read(&path)?;
    if current_disk.access != before.access || current_disk.refresh != before.refresh {
        return current_disk.access.ok_or_else(|| {
            LiveError::new(
                LimitIssueKind::Network,
                "Claude credentials changed during token refresh",
            )
        });
    }
    let mut root = current_disk.root;
    let oauth = root
        .get_mut("claudeAiOauth")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            LiveError::new(
                LimitIssueKind::InvalidResponse,
                "Claude credentials are incomplete",
            )
        })?;
    oauth.insert("accessToken".into(), Value::String(access.to_string()));
    oauth.insert(
        "refreshToken".into(),
        Value::String(
            response
                .get("refresh_token")
                .and_then(Value::as_str)
                .unwrap_or(refresh)
                .to_string(),
        ),
    );
    oauth.insert(
        "expiresAt".into(),
        json!(now_ms.saturating_add(u128::from(expires_in) * 1_000)),
    );
    if let Some(seconds) = response
        .get("refresh_token_expires_in")
        .and_then(Value::as_u64)
    {
        oauth.insert(
            "refreshTokenExpiresAt".into(),
            json!(now_ms.saturating_add(u128::from(seconds) * 1_000)),
        );
    }
    let bytes = serde_json::to_vec(&root).map_err(super::credentials::storage_error)?;
    crate::storage::write_private_atomic(&path, &bytes, "Claude credentials")
        .map_err(super::credentials::storage_error)?;
    Ok(access.to_string())
}

fn usable_without_refresh(
    credentials: &super::claude_credentials::ClaudeCredentials,
    now_ms: u128,
    force: bool,
    rejected_access: Option<&str>,
) -> bool {
    if force {
        return credentials
            .access
            .as_deref()
            .is_some_and(|access| rejected_access.is_some_and(|rejected| rejected != access));
    }
    credentials.access.is_some()
        && credentials
            .expires_at
            .is_some_and(|expiry| expiry > now_ms.saturating_add(REFRESH_SKEW_MS))
}

fn refresh_request(endpoint: &str, refresh: &str, scopes: &[String]) -> Result<Value, LiveError> {
    let mut body = json!({
        "grant_type": "refresh_token",
        "refresh_token": refresh,
        "client_id": CLIENT_ID,
    });
    if !scopes.is_empty() {
        body["scope"] = Value::String(scopes.join(" "));
    }
    get_json(|client| client.post(endpoint).json(&body)).map_err(classify_refresh_error)
}

fn classify_refresh_error(mut error: LiveError) -> LiveError {
    if error.error_code.as_deref() == Some("invalid_grant") {
        error.issue.kind = LimitIssueKind::Authentication;
        error.issue.message = "Claude sign-in needs to be renewed".into();
    } else if error.status == Some(StatusCode::TOO_MANY_REQUESTS) {
        error.issue.kind = LimitIssueKind::RateLimited;
    } else {
        error.issue.kind = LimitIssueKind::Network;
        error.issue.message = "Claude token refresh is temporarily unavailable".into();
    }
    error
}

fn now_millis() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
}
