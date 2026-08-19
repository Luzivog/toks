use serde::{Deserialize, Serialize};

use crate::accounts::{AccountId, CredentialProfileId, ProviderAccount};
use crate::Provider;

/// Exact local credential profiles backing one displayed logical account.
/// Callers obtain these IDs from account discovery, never from email or paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRemovalPlan {
    pub provider: Provider,
    pub logical_account_id: AccountId,
    pub local_profile_ids: Vec<CredentialProfileId>,
}

impl AccountRemovalPlan {
    pub fn from_account(provider: Provider, account: &ProviderAccount) -> Self {
        Self {
            provider,
            logical_account_id: account.id.clone(),
            local_profile_ids: account
                .sources
                .iter()
                .map(|source| source.profile_id.clone())
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ManagedRemovalState {
    Removed,
    AlreadyRemoved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ManagedProfileRemoval {
    pub local_profile_id: CredentialProfileId,
    pub state: ManagedRemovalState,
}

/// Filesystem work is complete when this is returned. Current provider
/// profiles still listed here must be suppressed by the account catalog.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRemovalResult {
    pub provider: Provider,
    pub logical_account_id: AccountId,
    pub managed_profiles: Vec<ManagedProfileRemoval>,
    pub hide_current_profile_ids: Vec<CredentialProfileId>,
    /// Live memos, limit snapshots, and pending operations for these exact
    /// local profiles must be invalidated by their owning subsystems.
    pub invalidate_local_profile_ids: Vec<CredentialProfileId>,
    pub history_retained: bool,
}

impl AccountRemovalResult {
    pub fn requires_catalog_suppression(&self) -> bool {
        !self.hide_current_profile_ids.is_empty()
    }
}
