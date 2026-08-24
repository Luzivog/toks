use crate::accounts::AccountId;

use super::super::{RotationEventKind, RotationRuntime, UnixMillis};

type AuthFailure = (u64, Option<UnixMillis>, Option<String>);

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
        state.auth_failure_revision = state.auth_failure_revision.saturating_add(1);
        state.auth_failed_at = Some(state.auth_failed_at.map_or(at, |previous| previous.max(at)));
        if let Some(fingerprint) = rejected_fingerprint {
            state.rejected_credential_fingerprint = Some(fingerprint.to_owned());
            state.remember_rejected_credential(fingerprint);
        }
        if std::mem::replace(&mut state.needs_sign_in, true) {
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
            .and_then(|state| state.auth_failure())
    }

    pub(crate) fn credential_was_rejected(&self, account: &AccountId, fingerprint: &str) -> bool {
        self.accounts
            .get(account)
            .is_some_and(|state| state.credential_was_rejected(fingerprint))
    }

    pub(crate) fn sign_in_restored_by_proof(
        &mut self,
        account: &AccountId,
        expected: AuthFailure,
        credential_fingerprint: &str,
    ) -> bool {
        self.accounts.get_mut(account).is_some_and(|state| {
            if state.auth_failure() != Some(expected.clone())
                || state.credential_was_rejected(credential_fingerprint)
                || expected
                    .2
                    .as_deref()
                    .is_some_and(|rejected| rejected == credential_fingerprint)
            {
                return false;
            }
            restore(state);
            true
        })
    }
}

fn restore(state: &mut super::super::AccountRuntime) {
    state.needs_sign_in = false;
    state.rejected_credential_fingerprint = None;
}
