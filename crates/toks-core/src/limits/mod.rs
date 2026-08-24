//! Plan-limit snapshots for supported providers.
//!
//! Providers discover limit windows from source data and map them into the
//! generic [`LimitWindow`], so new provider windows need no code update.

pub mod claude;
mod claude_auth;
mod claude_credentials;
mod claude_lock;
pub mod codex;
mod credentials;
mod http;
pub mod live;
mod live_fetch;
mod model;
mod plan;
mod reset_credits;
pub(crate) mod settling;
mod snapshot_cache;
mod status;

#[cfg(test)]
mod claude_auth_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod settling_tests;
#[cfg(test)]
mod snapshot_cache_tests;

pub use model::{LimitSnapshot, LimitWindow, Provider};
pub use plan::PlanMultiplier;
pub(crate) use plan::{read_claude_plan, PlanDetails};
pub use reset_credits::{
    BankedResetAttempt, BankedResetCredit, BankedResetCreditStatus, BankedResetOutcome,
};
pub use status::{LimitIssue, LimitIssueKind, SnapshotFreshness, SnapshotStatus};

pub(crate) fn forget_account_profile(
    provider: Provider,
    profile_id: &crate::accounts::CredentialProfileId,
) {
    live::forget_profile(provider, profile_id);
    let _ = snapshot_cache::remove_for_profile(provider, profile_id);
}

#[cfg(test)]
pub(crate) fn fetch_codex_for_test(
    profile: &crate::accounts::AccountProfile,
    url: &str,
) -> Result<live_fetch::LiveFetch, http::LiveError> {
    live_fetch::fetch_codex_for_test(profile, url)
}

#[cfg(test)]
pub(crate) fn cached_snapshot_for_test(
    profile: &crate::accounts::AccountProfile,
) -> Option<LimitSnapshot> {
    snapshot_cache::load(profile)
}

/// Prettify a raw identifier: `weekly_all` → `Weekly all`.
fn humanize_id(id: &str) -> String {
    let s = id.replace(['_', '-'], " ");
    let mut chars = s.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => s,
    }
}

fn parse_rfc3339(v: &serde_json::Value) -> Option<chrono::DateTime<chrono::Utc>> {
    v.as_str()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&chrono::Utc))
}

/// Read every discovered account from the live provider endpoints (throttled,
/// see [`live`]). Successful reads replace Toks's per-account snapshot;
/// the last snapshot remains available when a provider is temporarily down.
pub fn collect_all() -> Vec<LimitSnapshot> {
    crate::accounts::collect_limits()
}

/// Read Toks-owned and provider-owned snapshots without making a network
/// request. Apps should publish this result before starting a refresh.
pub fn hydrate_all() -> Vec<LimitSnapshot> {
    crate::accounts::hydrate_limits()
}
