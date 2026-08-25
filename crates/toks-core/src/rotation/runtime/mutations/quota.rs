use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::rotation::QuotaObservation;

use crate::rotation::runtime::{QuotaDrainState, RotationEventKind, RotationRuntime, UnixMillis};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

impl RotationRuntime {
    /// A confirmed provider redemption overrides the router's old reset time.
    pub fn banked_reset_consumed(
        &mut self,
        account: &AccountId,
        acknowledged_at: UnixMillis,
    ) -> bool {
        let state = self.accounts.entry(account.clone()).or_default();
        let acknowledged_at = state
            .reset_acknowledged_at
            .map_or(acknowledged_at, |current| current.max(acknowledged_at));
        let changed = (state.reset_acknowledged_at != Some(acknowledged_at))
            | state.blocked_until.take().is_some()
            | state.block_confirmed
            | state.block_reset_known
            | state.quota_drain.take().is_some()
            | !state.grandfathered_threads.is_empty()
            | !state.provisional_threads.is_empty()
            | !state.thread_usage.is_empty();
        state.reset_acknowledged_at = Some(acknowledged_at);
        state.block_confirmed = false;
        state.block_reset_known = false;
        state.grandfathered_threads.clear();
        state.provisional_threads.clear();
        state.thread_usage.clear();
        state.advance_quota_authority();
        changed
    }

    pub(crate) fn apply_quota_observations(
        &mut self,
        observations: &BTreeMap<AccountId, QuotaObservation>,
        at: UnixMillis,
    ) -> bool {
        let attached = self.attached_threads.clone();
        let mut changed = false;
        let mut newly_draining = Vec::new();
        for (account, state) in &mut self.accounts {
            let observation = observations
                .get(account)
                .copied()
                .unwrap_or(QuotaObservation::Unknown);
            if observation == QuotaObservation::Unknown {
                continue;
            }
            state.advance_quota_authority();
            changed = true;
            let reset_at = match observation {
                QuotaObservation::Unknown => unreachable!("unknown observations continue above"),
                QuotaObservation::ObservedAvailable => {
                    changed |= state.quota_drain.take().is_some();
                    changed |= state.blocked_until.take().is_some();
                    changed |= state.block_confirmed;
                    changed |= state.block_reset_known;
                    changed |= !state.grandfathered_threads.is_empty();
                    changed |= !state.provisional_threads.is_empty();
                    changed |= !state.thread_usage.is_empty();
                    state.block_confirmed = false;
                    state.block_reset_known = false;
                    state.grandfathered_threads.clear();
                    state.provisional_threads.clear();
                    state.thread_usage.clear();
                    continue;
                }
                QuotaObservation::Draining(reset_at) => reset_at,
            };
            // Before draining existed, the threshold was persisted as an
            // unconfirmed block. Convert it on the first current observation.
            if !state.block_confirmed {
                changed |= state.blocked_until.take().is_some();
                state.block_reset_known = false;
            }
            let previous = state.quota_drain;
            let next = match reset_at {
                Some(until) if until > at => Some(QuotaDrainState {
                    until,
                    reset_known: true,
                }),
                Some(_) => None,
                None => match previous {
                    Some(current) if !current.reset_known && current.until > at => Some(current),
                    // `until` is the next provider re-probe deadline when the
                    // reset is unknown. Renew it only after that deadline; the
                    // account remains draining while the re-probe runs.
                    _ => Some(QuotaDrainState {
                        until: UnixMillis::new(at.get().saturating_add(REPROBE_AFTER_MILLIS)),
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
