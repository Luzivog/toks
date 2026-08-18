use std::fs;
use std::path::Path;

use serde_json::Value;

use super::http::LiveError;
use super::LimitIssueKind;

pub(crate) struct ClaudeCredentials {
    pub(crate) root: Value,
    pub(crate) access: Option<String>,
    pub(crate) refresh: Option<String>,
    pub(crate) expires_at: Option<u128>,
    pub(crate) scopes: Vec<String>,
}

pub(crate) fn read(path: &Path) -> Result<ClaudeCredentials, LiveError> {
    let raw = fs::read_to_string(path).map_err(|error| {
        let kind = if error.kind() == std::io::ErrorKind::NotFound {
            LimitIssueKind::Authentication
        } else {
            LimitIssueKind::Storage
        };
        LiveError::new(kind, "Claude credentials could not be read")
    })?;
    let root: Value = serde_json::from_str(&raw).map_err(|_| {
        LiveError::new(
            LimitIssueKind::InvalidResponse,
            "Claude credentials are not valid JSON",
        )
    })?;
    let oauth = root
        .get("claudeAiOauth")
        .and_then(Value::as_object)
        .ok_or_else(|| {
            LiveError::new(
                LimitIssueKind::InvalidResponse,
                "Claude credentials are incomplete",
            )
        })?;
    let access = oauth
        .get("accessToken")
        .and_then(Value::as_str)
        .filter(|token| !token.is_empty())
        .map(str::to_string);
    Ok(ClaudeCredentials {
        access,
        refresh: oauth
            .get("refreshToken")
            .and_then(Value::as_str)
            .map(str::to_string),
        expires_at: oauth
            .get("expiresAt")
            .and_then(Value::as_u64)
            .map(u128::from),
        scopes: oauth
            .get("scopes")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        root,
    })
}
