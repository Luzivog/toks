use chrono::{DateTime, Duration, Utc};
use tokscope_core::{
    accounts::{
        AccountId, AccountIdentityKind, AccountOrderKey, AccountSource, CredentialProfileId,
        CredentialProfileKind,
    },
    LimitSnapshot, Provider, ProviderAccount,
};

const SIGN_IN_TIMEOUT: Duration = Duration::minutes(5);
mod evaluation;
mod pending;
use evaluation::{mark_pending, outcome, Outcome};
use pending::{AccountKey, OperationKind, PendingOperation};

#[derive(Clone, Debug)]
pub(crate) struct AccountOperationError {
    pub(crate) id: u64,
    pub(crate) message: String,
}

#[derive(Default)]
pub(crate) struct AccountOperations {
    pending: Vec<PendingOperation>,
    errors: Vec<AccountOperationError>,
    next_error_id: u64,
}

impl AccountOperations {
    pub(crate) fn authenticated_accounts(&self) -> Vec<AccountOrderKey> {
        self.pending
            .iter()
            .filter(|pending| {
                matches!(
                    tokscope_core::accounts::login_outcome(
                        pending.key.provider,
                        &pending.key.profile_id,
                    ),
                    Some(
                        tokscope_core::accounts::LoginOutcome::Authenticated
                            | tokscope_core::accounts::LoginOutcome::IdentityChanged
                    )
                )
            })
            .filter_map(|pending| {
                tokscope_core::accounts::unhide_profile(
                    pending.key.provider,
                    &pending.key.profile_id,
                )
                .ok()
                .flatten()
                .map(|account_id| AccountOrderKey::new(pending.key.provider, account_id.as_str()))
            })
            .collect()
    }

    pub(crate) fn start_add(
        &mut self,
        provider: Provider,
        profile_id: CredentialProfileId,
        started_at: DateTime<Utc>,
        limits: &mut Vec<LimitSnapshot>,
    ) {
        if !limits.iter().any(|snapshot| {
            snapshot.provider == provider
                && snapshot
                    .account
                    .sources
                    .iter()
                    .any(|source| source.profile_id == profile_id)
        }) {
            limits.push(LimitSnapshot::loading_account(
                provider,
                ProviderAccount {
                    id: AccountId::new(format!("{}-profile-{}", provider.slug(), profile_id)),
                    identity_kind: AccountIdentityKind::ProfileFallback,
                    email: None,
                    sources: vec![AccountSource {
                        profile_id: profile_id.clone(),
                        kind: CredentialProfileKind::Managed,
                        primary: true,
                    }],
                },
            ));
            tokscope_core::accounts::apply_saved_order(limits);
        }
        self.start(provider, profile_id, OperationKind::Add, started_at, false);
    }

    pub(crate) fn start_reauthentication(
        &mut self,
        provider: Provider,
        profile_id: CredentialProfileId,
        started_at: DateTime<Utc>,
    ) {
        self.start(
            provider,
            profile_id,
            OperationKind::Reauthenticate,
            started_at,
            true,
        );
    }

    fn start(
        &mut self,
        provider: Provider,
        profile_id: CredentialProfileId,
        kind: OperationKind,
        started_at: DateTime<Utc>,
        observed: bool,
    ) {
        self.pending.retain(|pending| {
            pending.key.provider != provider || pending.key.profile_id != profile_id
        });
        self.pending.push(PendingOperation {
            key: AccountKey {
                provider,
                profile_id,
            },
            kind,
            started_at,
            observed,
            missing_refreshes: 0,
        });
    }

    pub(crate) fn report_error(&mut self, message: String) {
        self.next_error_id = self.next_error_id.wrapping_add(1);
        self.errors.push(AccountOperationError {
            id: self.next_error_id,
            message,
        });
    }

    pub(crate) fn errors(&self) -> &[AccountOperationError] {
        &self.errors
    }

    pub(crate) fn dismiss_error(&mut self, id: u64) {
        self.errors.retain(|error| error.id != id);
    }

    pub(crate) fn reconcile(&mut self, snapshots: &mut [LimitSnapshot], now: DateTime<Utc>) {
        let mut failures = Vec::new();
        self.pending.retain_mut(|pending| {
            let snapshot = snapshots
                .iter()
                .find(|snapshot| pending.key.matches(snapshot));
            if snapshot.is_some() {
                pending.observed = true;
                pending.missing_refreshes = 0;
            } else {
                pending.missing_refreshes = pending.missing_refreshes.saturating_add(1);
            }
            match outcome(pending, snapshot, now) {
                Outcome::Pending => true,
                Outcome::Complete => false,
                Outcome::Failed(message) => {
                    failures.push(message);
                    false
                }
            }
        });
        for message in failures {
            self.report_error(message);
        }
        for pending in &self.pending {
            if let Some(snapshot) = snapshots
                .iter_mut()
                .find(|snapshot| pending.key.matches(snapshot))
            {
                mark_pending(snapshot, pending.started_at);
            }
        }
    }
}

#[cfg(test)]
mod tests;
