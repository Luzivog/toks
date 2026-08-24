use serde_json::Value;

use super::http::{get_json, LiveError};
use super::{claude, LimitIssueKind, LimitSnapshot, Provider};
use crate::accounts::{AccountProfile, CodexAuthProof};

mod codex_live;
#[cfg(test)]
pub(crate) use codex_live::fetch_codex_for_test;
pub(super) use codex_live::{codex_request_with_method, codex_tokens};

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
            let (snapshot, proof) = codex_live::fetch(profile)?;
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
