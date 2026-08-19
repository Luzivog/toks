use serde::{Deserialize, Serialize};

use crate::limits::Provider;

use super::super::{AccountId, CredentialProfileId};

pub(super) const DOCUMENT_VERSION: u8 = 1;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SuppressionDocument {
    pub(super) version: u8,
    pub(super) accounts: Vec<SuppressedAccount>,
}

impl Default for SuppressionDocument {
    fn default() -> Self {
        Self {
            version: DOCUMENT_VERSION,
            accounts: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct SuppressedAccount {
    pub(super) provider: Provider,
    pub(super) account_id: AccountId,
    /// Current profiles act as aliases only while their principal is missing.
    pub(super) current_profile_ids: Vec<CredentialProfileId>,
}

impl SuppressionDocument {
    pub(super) fn normalize(&mut self) {
        self.accounts.sort_by(|left, right| {
            (left.provider, &left.account_id).cmp(&(right.provider, &right.account_id))
        });
        self.accounts.dedup_by(|left, right| {
            left.provider == right.provider && left.account_id == right.account_id
        });
        for account in &mut self.accounts {
            account.current_profile_ids.sort();
            account.current_profile_ids.dedup();
        }
    }
}
