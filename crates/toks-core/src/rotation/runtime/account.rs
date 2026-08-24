use crate::rotation::{ThreadId, UnixMillis};
use serde::{Deserialize, Serialize};

use super::{AccountAvailability, AccountRuntime};

const REJECTED_CREDENTIAL_HISTORY_LIMIT: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum ThreadUsage {
    StandardOnly { until: UnixMillis },
    Blocked { until: UnixMillis },
}

impl AccountRuntime {
    pub(super) fn has_durable_routing_state(&self) -> bool {
        self != &Self::default()
    }

    pub(super) fn quota_authority_revision(&self) -> u64 {
        self.quota_authority_revision
    }

    pub(super) fn advance_quota_authority(&mut self) {
        self.quota_authority_revision = self.quota_authority_revision.wrapping_add(1);
    }

    pub fn blocked_until(&self) -> Option<UnixMillis> {
        self.blocked_until
    }

    pub fn needs_sign_in(&self) -> bool {
        self.needs_sign_in
    }

    pub(super) fn auth_failure(&self) -> Option<(u64, Option<super::UnixMillis>, Option<String>)> {
        self.needs_sign_in.then(|| {
            (
                self.auth_failure_revision,
                self.auth_failed_at,
                self.rejected_credential_fingerprint.clone(),
            )
        })
    }

    pub(super) fn remember_rejected_credential(&mut self, fingerprint: &str) {
        self.rejected_credential_history
            .retain(|rejected| rejected != fingerprint);
        self.rejected_credential_history
            .push_back(fingerprint.to_owned());
        while self.rejected_credential_history.len() > REJECTED_CREDENTIAL_HISTORY_LIMIT {
            self.rejected_credential_history.pop_front();
        }
    }

    pub(super) fn credential_was_rejected(&self, fingerprint: &str) -> bool {
        self.rejected_credential_history
            .iter()
            .any(|rejected| rejected == fingerprint)
    }

    pub(super) fn normalize_rejected_credentials(&mut self) {
        let current = self.rejected_credential_fingerprint.clone();
        let history = std::mem::take(&mut self.rejected_credential_history);
        for fingerprint in history.iter().chain(current.iter()) {
            self.remember_rejected_credential(fingerprint);
        }
    }

    pub fn availability(&self, now: UnixMillis) -> AccountAvailability {
        if self.needs_sign_in {
            return AccountAvailability::NeedsSignIn;
        }
        if let Some(until) = self.blocked_until.filter(|until| *until > now) {
            return AccountAvailability::Blocked {
                until,
                reset_known: self.block_reset_known,
            };
        }
        self.quota_drain
            .filter(|drain| !drain.reset_known || drain.until > now)
            .map_or(AccountAvailability::Available, |drain| {
                AccountAvailability::Draining {
                    until: drain.until,
                    reset_known: drain.reset_known,
                }
            })
    }

    pub(super) fn can_drain(&self, thread: &ThreadId, now: UnixMillis) -> bool {
        matches!(
            self.availability(now),
            AccountAvailability::Draining { .. } | AccountAvailability::Blocked { .. }
        ) && self.grandfathered_threads.contains(thread)
            && !matches!(
                self.thread_usage.get(thread),
                Some(ThreadUsage::Blocked { until }) if *until > now
            )
    }

    pub(super) fn requires_standard_tier(&self, thread: &ThreadId, now: UnixMillis) -> bool {
        matches!(
            self.thread_usage.get(thread),
            Some(ThreadUsage::StandardOnly { until }) if *until > now
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountRuntime, REJECTED_CREDENTIAL_HISTORY_LIMIT};

    #[test]
    fn rejected_credential_history_is_bounded_and_keeps_most_recent_unique_entries() {
        let mut account = AccountRuntime::default();
        for index in 0..(REJECTED_CREDENTIAL_HISTORY_LIMIT + 2) {
            account.remember_rejected_credential(&format!("fingerprint-{index}"));
        }

        account.remember_rejected_credential("fingerprint-2");

        assert_eq!(
            account.rejected_credential_history.len(),
            REJECTED_CREDENTIAL_HISTORY_LIMIT
        );
        assert!(!account.credential_was_rejected("fingerprint-0"));
        assert!(!account.credential_was_rejected("fingerprint-1"));
        assert!(account.credential_was_rejected("fingerprint-2"));
        assert!(account.credential_was_rejected(&format!(
            "fingerprint-{}",
            REJECTED_CREDENTIAL_HISTORY_LIMIT + 1
        )));
    }
}
