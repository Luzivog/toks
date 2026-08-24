use std::collections::BTreeSet;

use crate::accounts::AccountId;

use super::{account::ThreadUsage, RotationRuntime, UnixMillis};

impl RotationRuntime {
    /// Drop vanished account snapshots, create state for new accounts, and
    /// clear elapsed blocks. Transport and task ownership are reconciled
    /// separately and survive temporary discovery loss.
    pub fn reconcile(&mut self, discovered: &[AccountId], now: UnixMillis) -> bool {
        let before = self.accounts.clone();
        let attachments_before = self.attached_threads.clone();
        let known: BTreeSet<_> = discovered.iter().cloned().collect();
        // Discovery omission is not evidence that authentication, quota, or
        // credential history recovered. Dormant state remains ineligible
        // because selection also requires current discovery.
        self.accounts
            .retain(|account, state| known.contains(account) || state.has_durable_routing_state());
        self.attached_threads
            .retain(|_, attachment| attachment.connections() > 0);
        for account in discovered {
            let state = self.accounts.entry(account.clone()).or_default();
            let mut quota_authority_changed = false;
            if state
                .quota_drain
                .is_some_and(|drain| drain.reset_known && drain.until <= now)
            {
                state.quota_drain = None;
                quota_authority_changed = true;
            }
            if state.blocked_until.is_some_and(|until| until <= now) {
                state.blocked_until = None;
                state.block_confirmed = false;
                state.block_reset_known = false;
                quota_authority_changed = true;
            }
            let thread_usage_before = state.thread_usage.len();
            state.thread_usage.retain(|_, usage| match usage {
                ThreadUsage::StandardOnly { until } | ThreadUsage::Blocked { until } => {
                    *until > now
                }
            });
            quota_authority_changed |= state.thread_usage.len() != thread_usage_before;
            if state.quota_drain.is_none() && !state.block_confirmed {
                state.grandfathered_threads.clear();
                state.provisional_threads.clear();
                state.thread_usage.clear();
            }
            if quota_authority_changed {
                state.advance_quota_authority();
            }
        }
        let accounts_changed = self.accounts != before;
        let attachments_changed = self.attached_threads != attachments_before;
        let active_threads_changed = self.reconcile_active_threads(&known, now);
        accounts_changed || attachments_changed || active_threads_changed
    }
}
