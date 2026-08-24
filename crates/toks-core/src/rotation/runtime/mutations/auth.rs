use crate::accounts::AccountId;

use super::super::{account_auth::AuthFailure, RotationEventKind, RotationRuntime, UnixMillis};

impl RotationRuntime {
    pub fn auth_failed(&mut self, account: &AccountId, at: UnixMillis) -> bool {
        self.auth_failed_for_credential(account, at, None)
    }

    pub(crate) fn auth_failed_for_credential(
        &mut self,
        account: &AccountId,
        at: UnixMillis,
        rejected_fingerprint: Option<&str>,
    ) -> bool {
        let state = self.accounts.entry(account.clone()).or_default();
        if !state.auth.record_failure(at, rejected_fingerprint) {
            return false;
        }
        self.push_event(
            at,
            RotationEventKind::AuthNeeded {
                account_id: account.clone(),
            },
        );
        true
    }

    pub(crate) fn auth_failure(&self, account: &AccountId) -> Option<AuthFailure> {
        self.accounts
            .get(account)
            .and_then(|state| state.auth.failure())
    }

    pub(crate) fn credential_was_rejected(&self, account: &AccountId, fingerprint: &str) -> bool {
        self.accounts
            .get(account)
            .is_some_and(|state| state.auth.credential_was_rejected(fingerprint))
    }

    pub(crate) fn sign_in_restored_by_proof(
        &mut self,
        account: &AccountId,
        expected: AuthFailure,
        credential_fingerprint: &str,
    ) -> bool {
        self.accounts.get_mut(account).is_some_and(|state| {
            state
                .auth
                .restore_by_proof(expected, credential_fingerprint)
        })
    }
}
