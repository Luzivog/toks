//! Durable visibility decisions for logical provider accounts.
//!
//! The persisted document contains only Tokscope's opaque logical account IDs
//! and local credential-profile IDs. Provider principals and emails never
//! cross this seam.

mod filtering;
mod model;
mod store;

use anyhow::Result;

use crate::limits::{LimitSnapshot, Provider};

use super::{
    discover_profiles, AccountId, AccountIdentityKind, AccountProfile, CredentialProfileId,
    ProviderAccount,
};
use store::SuppressionStore;

/// Hide a logical account. Stale snapshots from every source known at removal
/// remain hidden, while a later managed source can explicitly restore it.
pub fn hide_account(provider: Provider, account: &ProviderAccount) -> Result<()> {
    SuppressionStore::default()?.hide(provider, account)
}

/// Restore an account after an explicit successful Add-account flow.
pub fn unhide_account(provider: Provider, account_id: &AccountId) -> Result<bool> {
    SuppressionStore::default()?.unhide(provider, account_id)
}

/// Resolve a successfully authenticated local profile to its opaque provider
/// principal, then restore that logical account. Fallback identities are never
/// allowed to unhide an account.
pub fn unhide_profile(
    provider: Provider,
    profile_id: &CredentialProfileId,
) -> Result<Option<AccountId>> {
    unhide_profile_from(
        &SuppressionStore::default()?,
        provider,
        profile_id,
        discover_profiles(),
    )
}

fn unhide_profile_from(
    store: &SuppressionStore,
    provider: Provider,
    profile_id: &CredentialProfileId,
    profiles: Vec<AccountProfile>,
) -> Result<Option<AccountId>> {
    let Some(account) = profiles
        .into_iter()
        .find(|profile| profile.provider == provider && &profile.profile_id == profile_id)
        .map(|profile| profile.account)
    else {
        return Ok(None);
    };
    if account.identity_kind != AccountIdentityKind::ProviderPrincipal {
        return Ok(None);
    }
    store.unhide(provider, &account.id)?;
    Ok(Some(account.id))
}

/// Apply persisted visibility after logical snapshots have been coalesced.
///
/// Storage failure is deliberately fail-open: usage collection must remain
/// available if this optional preference file is unreadable.
pub(crate) fn filter_hidden_accounts(snapshots: Vec<LimitSnapshot>) -> Vec<LimitSnapshot> {
    let Ok(store) = SuppressionStore::default() else {
        return snapshots;
    };
    store.filter(snapshots)
}

#[cfg(test)]
mod tests;
