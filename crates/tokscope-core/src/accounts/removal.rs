//! One account-removal transaction across lifecycle and visibility state.

use anyhow::Result;

use crate::limits::{self, Provider};

use super::{hide_account, AccountRemovalPlan, AccountRemovalResult, ProviderAccount};

/// Remove one displayed logical account without touching durable usage history.
///
/// Exact managed credential profiles are removed by the lifecycle module.
/// Provider-owned current profiles are hidden, never signed out or deleted.
pub fn remove_from_tokscope(
    provider: Provider,
    account: &ProviderAccount,
) -> Result<AccountRemovalResult> {
    let plan = AccountRemovalPlan::from_account(provider, account);
    let result = super::lifecycle::remove_account(&plan)?;

    for profile_id in &result.invalidate_local_profile_ids {
        limits::forget_account_profile(provider, profile_id);
    }
    if result.requires_catalog_suppression() {
        hide_account(provider, account)?;
    }
    Ok(result)
}
