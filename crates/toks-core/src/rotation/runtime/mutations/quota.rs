use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::super::{QuotaExhaustionState, RotationEventKind, RotationRuntime, UnixMillis};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

impl RotationRuntime {
    pub fn replace_quota_exhaustion(
        &mut self,
        exhausted: &BTreeMap<AccountId, Option<UnixMillis>>,
        at: UnixMillis,
    ) -> bool {
        let attached = self.attached_threads.clone();
        let mut changed = false;
        let mut newly_draining = Vec::new();
        for (account, state) in &mut self.accounts {
            let Some(reset_at) = exhausted.get(account) else {
                changed |= state.quota_exhaustion.take().is_some();
                changed |= !state.grandfathered_threads.is_empty();
                state.grandfathered_threads.clear();
                continue;
            };
            // Before draining existed, snapshot exhaustion was persisted as an
            // unconfirmed block. Convert it on the first current observation.
            if !state.block_confirmed {
                changed |= state.blocked_until.take().is_some();
                state.block_reset_known = false;
            }
            let previous = state.quota_exhaustion;
            let next = match reset_at {
                Some(until) if *until > at => Some(QuotaExhaustionState {
                    until: *until,
                    reset_known: true,
                }),
                Some(_) => None,
                None => match previous {
                    Some(current) if !current.reset_known && current.until > at => Some(current),
                    // Allow one heartbeat to re-probe after the bounded fallback.
                    Some(current) if !current.reset_known => None,
                    _ => Some(QuotaExhaustionState {
                        until: UnixMillis::new(at.get() + REPROBE_AFTER_MILLIS),
                        reset_known: false,
                    }),
                },
            };
            changed |= previous != next;
            state.quota_exhaustion = next;
            if next.is_none() {
                changed |= !state.grandfathered_threads.is_empty();
                state.grandfathered_threads.clear();
            }
            if previous.is_none() && next.is_some() {
                newly_draining.push(account.clone());
            }
            if next.is_some()
                && !(state.block_confirmed && state.blocked_until.is_some_and(|until| until > at))
            {
                for (thread, attachment) in &attached {
                    if &attachment.account == account {
                        changed |= state.grandfathered_threads.insert(thread.clone());
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
