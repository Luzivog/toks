use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::limits::Provider;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAccount {
    /// Opaque local identifier. It is not a provider account ID.
    pub id: String,
    pub email: Option<String>,
}

impl ProviderAccount {
    pub fn unidentified_for(provider: Provider) -> Self {
        Self {
            id: format!("{}-unidentified", provider.slug()),
            email: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AddAccountStarted {
    pub provider: Provider,
    /// Stable local profile identifier for lifecycle tracking.
    pub account_id: String,
}

#[derive(Debug, Clone)]
pub(crate) struct AccountProfile {
    pub provider: Provider,
    pub account: ProviderAccount,
    /// Synthetic HOME for managed profiles; the real HOME for the current CLI profile.
    pub home_dir: PathBuf,
    /// CODEX_HOME or CLAUDE_CONFIG_DIR, depending on the provider.
    pub config_dir: PathBuf,
    pub managed: bool,
    /// Creation time for managed profiles. Current provider profiles predate
    /// Tokscope and therefore have no sign-in transition to model.
    pub created_at_ms: Option<u128>,
}

impl AccountProfile {
    pub(crate) fn cache_key(&self) -> String {
        format!("{}:{}", self.provider.slug(), self.account.id)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ProfileMetadata {
    pub(super) version: u8,
    pub(super) id: String,
    pub(super) provider: Provider,
    pub(super) created_at_ms: u128,
}
