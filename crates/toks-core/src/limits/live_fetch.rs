use std::time::Duration;

use serde_json::Value;

use super::http::{get_json, get_typed_json, LiveError};
use super::{claude, codex, LimitIssueKind, LimitSnapshot, Provider};
use crate::accounts::{AccountProfile, CodexAuthProof, CodexAuthSnapshot};

const CLAUDE_USAGE_URL: &str = "https://api.anthropic.com/api/oauth/usage";
const CLAUDE_BETA_HEADER: &str = "oauth-2025-04-20";

pub(crate) struct LiveFetch {
    pub(crate) snapshot: LimitSnapshot,
    pub(crate) codex_auth: Option<CodexAuthProof>,
}

pub(crate) fn fetch(profile: &AccountProfile) -> Result<LiveFetch, LiveError> {
    let (snapshot, codex_auth) = match profile.provider {
        Provider::Claude => (with_profile_identity(fetch_claude(profile)?, profile), None),
        Provider::Codex => {
            let (snapshot, proof) = fetch_codex(profile)?;
            (snapshot, Some(proof))
        }
    };
    Ok(LiveFetch {
        snapshot,
        codex_auth,
    })
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

const CODEX_USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
const RESET_CREDITS_TIMEOUT: Duration = Duration::from_secs(5);

fn fetch_codex(profile: &AccountProfile) -> Result<(LimitSnapshot, CodexAuthProof), LiveError> {
    fetch_codex_with(profile, CODEX_USAGE_URL, CodexAuthSnapshot::read)
}

fn fetch_codex_with(
    profile: &AccountProfile,
    url: &str,
    read: fn(&AccountProfile) -> Result<CodexAuthSnapshot, String>,
) -> Result<(LimitSnapshot, CodexAuthProof), LiveError> {
    let auth = read(profile).map_err(|_| {
        LiveError::new(
            LimitIssueKind::Authentication,
            "Codex sign-in is no longer valid",
        )
    })?;
    let proof = auth.proof();
    let value = get_json(|client| {
        codex_request(
            client,
            url,
            &auth.access_token,
            auth.chatgpt_account_id.as_deref(),
        )
    })?;
    let mut snapshot = codex::parse(&value, Some(chrono::Utc::now()), "live".into());
    if snapshot.banked_resets > 0 {
        set_reset_credit_details(
            &mut snapshot,
            fetch_reset_credits(&auth.access_token, auth.chatgpt_account_id.as_deref()).ok(),
        );
    }
    Ok((ensure_windows(snapshot)?, proof))
}

fn fetch_reset_credits(
    token: &str,
    account_id: Option<&str>,
) -> Result<codex::ResetCreditDetailsResponse, LiveError> {
    get_typed_json(|client| {
        codex_request(client, RESET_CREDITS_URL, token, account_id).timeout(RESET_CREDITS_TIMEOUT)
    })
}

fn codex_request(
    client: &reqwest::Client,
    url: &str,
    token: &str,
    account_id: Option<&str>,
) -> reqwest::RequestBuilder {
    codex_request_with_method(client, reqwest::Method::GET, url, token, account_id)
}

pub(crate) fn codex_request_with_method(
    client: &reqwest::Client,
    method: reqwest::Method,
    url: &str,
    token: &str,
    account_id: Option<&str>,
) -> reqwest::RequestBuilder {
    let mut request = client
        .request(method, url)
        .bearer_auth(token)
        .header("Accept", "application/json")
        .header(
            "User-Agent",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
        );
    if let Some(id) = account_id {
        request = request.header("ChatGPT-Account-Id", id);
    }
    request
}

pub(crate) fn codex_tokens(profile: &AccountProfile) -> Option<(String, Option<String>)> {
    let auth = CodexAuthSnapshot::read(profile).ok()?;
    Some((auth.access_token, auth.chatgpt_account_id))
}

pub(super) fn set_reset_credit_details(
    snapshot: &mut LimitSnapshot,
    details: Option<codex::ResetCreditDetailsResponse>,
) {
    snapshot.banked_reset_credits = details.map(codex::reset_credits_into_domain);
}

#[cfg(test)]
pub(crate) fn fetch_codex_for_test(
    profile: &AccountProfile,
    url: &str,
) -> Result<LiveFetch, LiveError> {
    let (snapshot, proof) =
        fetch_codex_with(profile, url, crate::accounts::read_codex_auth_for_test)?;
    Ok(LiveFetch {
        snapshot,
        codex_auth: Some(proof),
    })
}
