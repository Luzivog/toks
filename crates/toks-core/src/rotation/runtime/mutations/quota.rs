use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::super::{QuotaDrainState, RotationEventKind, RotationRuntime, UnixMillis};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

impl RotationRuntime {
    /// A confirmed provider redemption overrides the router's old reset time.
    pub fn banked_reset_consumed(&mut self, account: &AccountId) -> bool {
        let Some(state) = self.accounts.get_mut(account) else {
            return false;
        };
        let changed = state.blocked_until.take().is_some()
            | state.block_confirmed
            | state.block_reset_known
            | state.quota_drain.take().is_some()
            | !state.grandfathered_threads.is_empty()
            | !state.provisional_threads.is_empty()
            | !state.thread_usage.is_empty();
        state.block_confirmed = false;
        state.block_reset_known = false;
        state.grandfathered_threads.clear();
        state.provisional_threads.clear();
        state.thread_usage.clear();
        changed
    }

    pub fn replace_quota_drain(
        &mut self,
        draining: &BTreeMap<AccountId, Option<UnixMillis>>,
        at: UnixMillis,
    ) -> bool {
        let attached = self.attached_threads.clone();
        let mut changed = false;
        let mut newly_draining = Vec::new();
        for (account, state) in &mut self.accounts {
            let Some(reset_at) = draining.get(account) else {
                changed |= state.quota_drain.take().is_some();
                if !state.block_confirmed {
                    changed |= !state.grandfathered_threads.is_empty();
                    changed |= !state.provisional_threads.is_empty();
                    changed |= !state.thread_usage.is_empty();
                    state.grandfathered_threads.clear();
                    state.provisional_threads.clear();
                    state.thread_usage.clear();
                }
                continue;
            };
            // Before draining existed, the threshold was persisted as an
            // unconfirmed block. Convert it on the first current observation.
            if !state.block_confirmed {
                changed |= state.blocked_until.take().is_some();
                state.block_reset_known = false;
            }
            let previous = state.quota_drain;
            let next = match reset_at {
                Some(until) if *until > at => Some(QuotaDrainState {
                    until: *until,
                    reset_known: true,
                }),
                Some(_) => None,
                None => match previous {
                    Some(current) if !current.reset_known && current.until > at => Some(current),
                    // Allow one heartbeat to re-probe after the bounded fallback.
                    Some(current) if !current.reset_known => None,
                    _ => Some(QuotaDrainState {
                        until: UnixMillis::new(at.get() + REPROBE_AFTER_MILLIS),
                        reset_known: false,
                    }),
                },
            };
            changed |= previous != next;
            state.quota_drain = next;
            if next.is_none() {
                changed |= !state.grandfathered_threads.is_empty();
                changed |= !state.provisional_threads.is_empty();
                changed |= !state.thread_usage.is_empty();
                state.grandfathered_threads.clear();
                state.provisional_threads.clear();
                state.thread_usage.clear();
            }
            if previous.is_none() && next.is_some() {
                newly_draining.push(account.clone());
                for (thread, attachment) in &attached {
                    if &attachment.account == account {
                        changed |= state.grandfathered_threads.insert(thread.clone());
                    }
                }
                for (thread, active) in &self.active_threads {
                    if &active.account_id == account {
                        let inserted = state.grandfathered_threads.insert(thread.clone());
                        changed |= inserted;
                        if inserted && active.reservation_only() {
                            changed |= state.provisional_threads.insert(thread.clone());
                        }
                    }
                }
            }
        }
        for account_id in newly_draining {
            self.push_event(at, RotationEventKind::Draining { account_id });
        }
        changed
    }
}
