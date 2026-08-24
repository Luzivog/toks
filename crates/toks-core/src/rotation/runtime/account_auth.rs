use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::rotation::UnixMillis;

const REJECTED_CREDENTIAL_HISTORY_LIMIT: usize = 32;

pub(super) type AuthFailure = (u64, Option<UnixMillis>, Option<String>);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AccountAuthState {
    needs_sign_in: bool,
    #[serde(default)]
    auth_failure_revision: u64,
    #[serde(default)]
    auth_failed_at: Option<UnixMillis>,
    #[serde(default)]
    rejected_credential_fingerprint: Option<String>,
    #[serde(default)]
    rejected_credential_history: VecDeque<String>,
}

impl AccountAuthState {
    pub(super) fn needs_sign_in(&self) -> bool {
        self.needs_sign_in
    }

    pub(super) fn record_failure(
        &mut self,
        at: UnixMillis,
        rejected_fingerprint: Option<&str>,
    ) -> bool {
        self.auth_failure_revision = self.auth_failure_revision.saturating_add(1);
        self.auth_failed_at = Some(self.auth_failed_at.map_or(at, |previous| previous.max(at)));
        if let Some(fingerprint) = rejected_fingerprint {
            self.rejected_credential_fingerprint = Some(fingerprint.to_owned());
            self.remember_rejected_credential(fingerprint);
        }
        !std::mem::replace(&mut self.needs_sign_in, true)
    }

    pub(super) fn failure(&self) -> Option<AuthFailure> {
        self.needs_sign_in.then(|| {
            (
                self.auth_failure_revision,
                self.auth_failed_at,
                self.rejected_credential_fingerprint.clone(),
            )
        })
    }

    pub(super) fn credential_was_rejected(&self, fingerprint: &str) -> bool {
        self.rejected_credential_history
            .iter()
            .any(|rejected| rejected == fingerprint)
    }

    pub(super) fn restore_by_proof(
        &mut self,
        expected: AuthFailure,
        credential_fingerprint: &str,
    ) -> bool {
        if self.failure() != Some(expected.clone())
            || self.credential_was_rejected(credential_fingerprint)
            || expected
                .2
                .as_deref()
                .is_some_and(|rejected| rejected == credential_fingerprint)
        {
            return false;
        }
        self.needs_sign_in = false;
        self.rejected_credential_fingerprint = None;
        true
    }

    pub(super) fn normalize_rejected_credentials(&mut self) {
        let current = self.rejected_credential_fingerprint.clone();
        let history = std::mem::take(&mut self.rejected_credential_history);
        for fingerprint in history.iter().chain(current.iter()) {
            self.remember_rejected_credential(fingerprint);
        }
    }

    fn remember_rejected_credential(&mut self, fingerprint: &str) {
        self.rejected_credential_history
            .retain(|rejected| rejected != fingerprint);
        self.rejected_credential_history
            .push_back(fingerprint.to_owned());
        while self.rejected_credential_history.len() > REJECTED_CREDENTIAL_HISTORY_LIMIT {
            self.rejected_credential_history.pop_front();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountAuthState, REJECTED_CREDENTIAL_HISTORY_LIMIT};

    #[test]
    fn rejected_credential_history_is_bounded_and_keeps_most_recent_unique_entries() {
        let mut auth = AccountAuthState::default();
        for index in 0..(REJECTED_CREDENTIAL_HISTORY_LIMIT + 2) {
            auth.remember_rejected_credential(&format!("fingerprint-{index}"));
        }

        auth.remember_rejected_credential("fingerprint-2");

        assert_eq!(
            auth.rejected_credential_history.len(),
            REJECTED_CREDENTIAL_HISTORY_LIMIT
        );
        assert!(!auth.credential_was_rejected("fingerprint-0"));
        assert!(!auth.credential_was_rejected("fingerprint-1"));
        assert!(auth.credential_was_rejected("fingerprint-2"));
        assert!(auth.credential_was_rejected(&format!(
            "fingerprint-{}",
            REJECTED_CREDENTIAL_HISTORY_LIMIT + 1
        )));
    }
}
