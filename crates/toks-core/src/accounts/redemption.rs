use anyhow::{Context, Result};

use crate::{
    limits::{BankedResetAttempt, BankedResetOutcome, Provider},
    ProviderAccount,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BankedResetResult {
    pub outcome: BankedResetOutcome,
    pub routing_updated: bool,
}

/// Consume one Codex reset for this exact logical account. The caller owns the
/// attempt so a retry can reuse the same provider idempotency key.
pub fn redeem_banked_reset(
    account: &ProviderAccount,
    attempt: &BankedResetAttempt,
) -> Result<BankedResetResult> {
    let profiles: Vec<_> = super::discover_profiles()
        .into_iter()
        .filter(|profile| profile.provider == Provider::Codex && profile.account.id == account.id)
        .collect();
    let primary = account.primary_source().map(|source| &source.profile_id);
    let profile = primary
        .and_then(|primary| {
            profiles
                .iter()
                .find(|profile| &profile.profile_id == primary)
        })
        .or_else(|| profiles.first())
        .context("Codex account credentials are no longer available")?;

    let outcome = crate::limits::codex::redeem_banked_reset(profile, attempt)
        .map_err(|error| anyhow::anyhow!(error.issue.message))?;
    let routing_updated = if outcome.used_credit() {
        crate::limits::live::forget_profile(profile.provider, &profile.profile_id);
        crate::codex_router::acknowledge_banked_reset(&account.id).is_ok()
    } else {
        true
    };
    Ok(BankedResetResult {
        outcome,
        routing_updated,
    })
}
