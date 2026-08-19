use chrono::{DateTime, Utc};
use toks_core::{accounts::CredentialProfileId, LimitSnapshot, Provider};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AccountKey {
    pub(super) provider: Provider,
    pub(super) profile_id: CredentialProfileId,
}

impl AccountKey {
    pub(super) fn matches(&self, snapshot: &LimitSnapshot) -> bool {
        self.provider == snapshot.provider
            && snapshot
                .account
                .sources
                .iter()
                .any(|source| source.profile_id == self.profile_id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OperationKind {
    Add,
    Reauthenticate,
}

#[derive(Clone, Debug)]
pub(super) struct PendingOperation {
    pub(super) key: AccountKey,
    pub(super) kind: OperationKind,
    pub(super) started_at: DateTime<Utc>,
    pub(super) observed: bool,
    pub(super) missing_refreshes: u8,
}
