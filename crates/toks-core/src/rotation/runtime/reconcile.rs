use std::collections::BTreeSet;

use crate::accounts::AccountId;

use super::{account::ThreadUsage, RotationRuntime, UnixMillis};

impl RotationRuntime {
    /// Drop vanished accounts, create state for new accounts, and clear
    /// elapsed blocks. This mutation never touches waiting threads.
    pub fn reconcile(&mut self, discovered: &[AccountId], now: UnixMillis) -> bool {
        let before = self.accounts.clone();
        let known: BTreeSet<_> = discovered.iter().cloned().collect();
        self.accounts.retain(|account, _| known.contains(account));
        for account in discovered {
            let state = self.accounts.entry(account.clone()).or_default();
            if state.quota_drain.is_some_and(|drain| drain.until <= now) {
                state.quota_drain = None;
            }
            if state.blocked_until.is_some_and(|until| until <= now) {
                state.blocked_until = None;
                state.block_confirmed = false;
                state.block_reset_known = false;
            }
            state.thread_usage.retain(|_, usage| match usage {
                ThreadUsage::StandardOnly { until } | ThreadUsage::Blocked { until } => {
                    *until > now
                }
            });
            if state.quota_drain.is_none() && !state.block_confirmed {
                state.grandfathered_threads.clear();
                state.provisional_threads.clear();
                state.thread_usage.clear();
            }
        }
        self.accounts != before || self.reconcile_active_threads(&known, now)
    }
}
