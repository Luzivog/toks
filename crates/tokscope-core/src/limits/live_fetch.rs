use serde_json::Value;

use super::http::{get_json, LiveError};
use super::{claude, codex, LimitIssueKind, LimitSnapshot, Provider};
use crate::accounts::AccountProfile;

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_BETA_HEADER: &str = "oauth-2025-04-20";
const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";

pub(crate) fn fetch(profile: &AccountProfile) -> Result<LimitSnapshot, LiveError> {
    let snapshot = match profile.provider {
        Provider::Claude => fetch_claude(profile),
        Provider::Codex => fetch_codex(profile),
    }?;
    Ok(with_profile_identity(snapshot, profile))
}

fn fetch_claude(profile: &AccountProfile) -> Result<LimitSnapshot, LiveError> {
    let mut token = super::claude_auth::access_token(profile)?;
    let value = match fetch_claude_usage(&token) {
        Err(error) if error.issue.kind == LimitIssueKind::Authentication => {
            token = super::claude_auth::refresh_after_rejection(profile, &token)?;
            fetch_claude_usage(&token)?
        }
        outcome => outcome?,
    };
    let mut snapshot = claude::parse_utilization(&value, Some(chrono::Utc::now()), "live".into());
    let details = super::read_claude_plan(&profile.config_dir);
    snapshot.plan = details.name;
    snapshot.plan_multiplier = details.multiplier.or(snapshot.plan_multiplier);
    ensure_windows(snapshot)
}

fn fetch_claude_usage(token: &str) -> Result<Value, LiveError> {
    get_json(|client| {
        client
            .get(CLAUDE_USAGE_URL)
            .bearer_auth(token)
            .header("Accept", "application/json")
            .header("anthropic-beta", CLAUDE_BETA_HEADER)
    })
}

fn fetch_codex(profile: &AccountProfile) -> Result<LimitSnapshot, LiveError> {
    let (token, account_id) = codex_tokens(profile).ok_or_else(|| {
        LiveError::new(
            LimitIssueKind::Authentication,
            "Codex sign-in is no longer valid",
        )
    })?;
    let value = get_json(|client| {
        let mut request = client
            .get(CODEX_USAGE_URL)
            .bearer_auth(&token)
            .header("Accept", "application/json")
            .header(
                "User-Agent",
                "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
            );
        if let Some(id) = &account_id {
            request = request.header("ChatGPT-Account-Id", id);
        }
        request
    })?;
    ensure_windows(codex::parse(
        &value,
        Some(chrono::Utc::now()),
        "live".into(),
    ))
}

fn ensure_windows(snapshot: LimitSnapshot) -> Result<LimitSnapshot, LiveError> {
    if snapshot.windows.is_empty() {
        Err(LiveError::new(
            LimitIssueKind::InvalidResponse,
            "provider response contained no limit windows",
        ))
    } else {
        Ok(snapshot)
    }
}

fn with_profile_identity(mut snapshot: LimitSnapshot, profile: &AccountProfile) -> LimitSnapshot {
    let response_email = snapshot.account.email.take();
    snapshot.account = profile.account.clone();
    snapshot.account.email = snapshot.account.email.or(response_email);
    snapshot
}

fn codex_tokens(profile: &AccountProfile) -> Option<(String, Option<String>)> {
    let raw = std::fs::read_to_string(profile.config_dir.join("auth.json")).ok()?;
    let value = serde_json::from_str::<Value>(&raw).ok()?;
    let token = value.pointer("/tokens/access_token")?.as_str()?.to_string();
    let account_id = value
        .pointer("/tokens/account_id")
        .and_then(Value::as_str)
        .map(str::to_string);
    Some((token, account_id))
}
