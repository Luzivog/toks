use std::collections::BTreeSet;

use crate::accounts::AccountId;
use crate::rotation::{BlockWindow, FastLimitDisposition, FastLimitOutcome, UsageLimitIncident};

use super::super::{
    account::ThreadUsage, AccountRuntime, RotationEventKind, RotationRuntime, ThreadId, UnixMillis,
};

impl RotationRuntime {
    pub(crate) fn usage_limited(
        &mut self,
        account: &AccountId,
        incident: UsageLimitIncident,
        at: UnixMillis,
    ) {
        self.push_event(
            at,
            RotationEventKind::UsageLimited {
                account_id: account.clone(),
                incident,
            },
        );
    }

    pub fn block_admission(
        &mut self,
        account: &AccountId,
        window: BlockWindow,
        at: UnixMillis,
    ) -> bool {
        let drainable = self.drainable_threads(account);
        let material_changed = {
            let state = self.accounts.entry(account.clone()).or_default();
            let before = state.clone();
            state.grandfathered_threads.extend(drainable);
            extend_admission_block(state, window);
            let changed = state != &before;
            state.advance_quota_authority();
            changed
        };
        if material_changed {
            self.push_event(
                at,
                RotationEventKind::Blocked {
                    account_id: account.clone(),
                    until: window.until(),
                },
            );
        }
        material_changed
    }

    pub fn thread_blocked(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        window: BlockWindow,
        at: UnixMillis,
    ) -> bool {
        let drainable = self.drainable_threads(account);
        let material_changed = {
            let state = self.accounts.entry(account.clone()).or_default();
            let before = state.clone();
            if state.quota_drain.is_none() {
                state.grandfathered_threads.extend(drainable);
            }
            extend_admission_block(state, window);
            let keep_later_block = matches!(
                state.thread_usage.get(thread),
                Some(ThreadUsage::Blocked { until }) if *until >= window.until()
            );
            if !keep_later_block {
                state.thread_usage.insert(
                    thread.clone(),
                    ThreadUsage::Blocked {
                        until: window.until(),
                    },
                );
            }
            let changed = state != &before;
            state.advance_quota_authority();
            changed
        };
        if material_changed {
            self.push_event(
                at,
                RotationEventKind::ThreadBlocked {
                    thread_id: thread.clone(),
                    account_id: account.clone(),
                    until: window.until(),
                },
            );
        }
        material_changed
    }

    pub(crate) fn fast_limit_reached(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        window: BlockWindow,
        disposition: FastLimitDisposition,
        at: UnixMillis,
    ) -> (FastLimitOutcome, bool) {
        let state = self.accounts.entry(account.clone()).or_default();
        state.advance_quota_authority();
        match state.thread_usage.get(thread) {
            Some(ThreadUsage::StandardOnly { until }) if *until > at => {
                return (FastLimitOutcome::UseStandard, false);
            }
            Some(ThreadUsage::Blocked { until }) if *until > at => {
                return (FastLimitOutcome::AlreadyBlocked, false);
            }
            None => {}
            Some(_) => {}
        }
        state.thread_usage.insert(
            thread.clone(),
            ThreadUsage::StandardOnly {
                until: window.until(),
            },
        );
        let event = match disposition {
            FastLimitDisposition::RetryingStandard => RotationEventKind::FastFallback {
                thread_id: thread.clone(),
                account_id: account.clone(),
            },
            FastLimitDisposition::NextRequestUsesStandard => RotationEventKind::FastUnavailable {
                thread_id: thread.clone(),
                account_id: account.clone(),
            },
        };
        self.push_event(at, event);
        (FastLimitOutcome::UseStandard, true)
    }

    fn drainable_threads(&self, account: &AccountId) -> BTreeSet<ThreadId> {
        self.attached_threads
            .iter()
            .filter(|(_, attachment)| &attachment.account == account)
            .map(|(thread, _)| thread.clone())
            .chain(
                self.active_threads
                    .iter()
                    .filter(|(_, active)| &active.account_id == account)
                    .map(|(thread, _)| thread.clone()),
            )
            .collect()
    }
}

fn extend_admission_block(state: &mut AccountRuntime, window: BlockWindow) {
    let until = window.until();
    match state.blocked_until {
        Some(current) if current > until => {}
        Some(current) if current == until => state.block_reset_known |= window.reset_known(),
        _ => {
            state.blocked_until = Some(until);
            state.block_reset_known = window.reset_known();
        }
    }
    state.block_confirmed = true;
}
