use std::time::Duration;

use super::ensure_windows;
#[cfg(test)]
use super::LiveFetch;
use crate::accounts::{AccountProfile, CodexAuthProof, CodexAuthSnapshot};
use crate::limits::http::{get_json, get_typed_json, LiveError};
use crate::limits::{codex, LimitIssueKind, LimitSnapshot};

const USAGE_URL: &str = "https://chatgpt.com/backend-api/wham/usage";
const RESET_CREDITS_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits";
const RESET_CREDITS_TIMEOUT: Duration = Duration::from_secs(5);

pub(super) fn fetch(
    profile: &AccountProfile,
) -> Result<(LimitSnapshot, CodexAuthProof), LiveError> {
    fetch_with(profile, USAGE_URL, CodexAuthSnapshot::read)
}

fn fetch_with(
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
        request(
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
        request(client, RESET_CREDITS_URL, token, account_id).timeout(RESET_CREDITS_TIMEOUT)
    })
}

fn request(
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

fn set_reset_credit_details(
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
    let (snapshot, proof) = fetch_with(profile, url, crate::accounts::read_codex_auth_for_test)?;
    Ok(LiveFetch {
        snapshot,
        codex_auth: Some(proof),
    })
}

#[cfg(test)]
mod tests {
    use super::set_reset_credit_details;

    #[test]
    fn missing_optional_details_preserve_the_usage_count() {
        let mut snapshot = snapshot();
        set_reset_credit_details(&mut snapshot, None);
        assert_eq!(snapshot.banked_resets, 3);
        assert_eq!(snapshot.banked_reset_credits, None);
    }

    #[test]
    fn detail_endpoint_count_cannot_override_the_usage_count() {
        let mut snapshot = snapshot();
        let details = serde_json::from_value(serde_json::json!({
            "available_count": 99,
            "credits": [{"status": "available"}]
        }))
        .unwrap();
        set_reset_credit_details(&mut snapshot, Some(details));
        assert_eq!(snapshot.banked_resets, 3);
        assert_eq!(snapshot.banked_reset_credits.unwrap().len(), 1);
    }

    fn snapshot() -> crate::limits::LimitSnapshot {
        crate::limits::codex::parse(
            &serde_json::json!({
                "rate_limit": {"primary_window": {
                    "used_percent": 50,
                    "limit_window_seconds": 3600
                }},
                "rate_limit_reset_credits": {"available_count": 3}
            }),
            None,
            "test".into(),
        )
    }
}
