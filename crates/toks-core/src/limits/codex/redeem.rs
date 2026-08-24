use reqwest::Method;
use serde::{Deserialize, Serialize};

use crate::{
    accounts::AccountProfile,
    limits::{BankedResetAttempt, BankedResetOutcome, LimitIssueKind},
};

use crate::limits::{
    http::{request_typed_json, LiveError},
    live_fetch::{codex_request_with_method, codex_tokens},
};

const CONSUME_URL: &str = "https://chatgpt.com/backend-api/wham/rate-limit-reset-credits/consume";

#[derive(Serialize)]
struct ConsumeRequest<'a> {
    redeem_request_id: &'a str,
}

#[derive(Deserialize)]
struct ConsumeResponse {
    code: ConsumeCode,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConsumeCode {
    Reset,
    NothingToReset,
    NoCredit,
    AlreadyRedeemed,
}

pub(crate) fn redeem_banked_reset(
    profile: &AccountProfile,
    attempt: &BankedResetAttempt,
) -> Result<BankedResetOutcome, LiveError> {
    let (token, account_id) = codex_tokens(profile).ok_or_else(|| {
        LiveError::new(
            LimitIssueKind::Authentication,
            "Codex sign-in is no longer valid",
        )
    })?;
    redeem_with_credentials(&token, account_id.as_deref(), attempt, CONSUME_URL)
}

pub(super) fn redeem_with_credentials(
    token: &str,
    account_id: Option<&str>,
    attempt: &BankedResetAttempt,
    url: &str,
) -> Result<BankedResetOutcome, LiveError> {
    let response: ConsumeResponse = request_typed_json(|client| {
        codex_request_with_method(client, Method::POST, url, token, account_id).json(
            &ConsumeRequest {
                redeem_request_id: attempt.request_id(),
            },
        )
    })?;
    Ok(match response.code {
        ConsumeCode::Reset => BankedResetOutcome::Reset,
        ConsumeCode::NothingToReset => BankedResetOutcome::NothingToReset,
        ConsumeCode::NoCredit => BankedResetOutcome::NoCredit,
        ConsumeCode::AlreadyRedeemed => BankedResetOutcome::AlreadyRedeemed,
    })
}
