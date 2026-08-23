use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationEventKind, RotationRuntime, ThreadId, UnixMillis};

const ABANDONED_FOLLOW_UP_MILLIS: i64 = 24 * 60 * 60 * 1_000;

mod reservations;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActiveThread {
    pub(super) account_id: AccountId,
    streams: u32,
    #[serde(default)]
    reservations: u32,
    awaiting_follow_up: bool,
    last_activity_at: UnixMillis,
}

impl RotationRuntime {
    pub fn connection_opened(&mut self, account: &AccountId, thread: &ThreadId, at: UnixMillis) {
        self.accounts.entry(account.clone()).or_default();
        if let Some(state) = self.accounts.get_mut(account) {
            state.provisional_threads.remove(thread);
        }
        let active = self
            .active_threads
            .entry(thread.clone())
            .or_insert_with(|| ActiveThread {
                account_id: account.clone(),
                streams: 0,
                reservations: 0,
                awaiting_follow_up: false,
                last_activity_at: at,
            });
        if &active.account_id != account {
            active.account_id = account.clone();
            active.streams = 0;
            active.reservations = 0;
        }
        active.reservations = active.reservations.saturating_sub(1);
        active.awaiting_follow_up = false;
        active.streams = active.streams.saturating_add(1);
        active.last_activity_at = at;
        self.push_event(
            at,
            RotationEventKind::Routed {
                thread_id: thread.clone(),
                account_id: account.clone(),
            },
        );
    }

    pub fn connection_closed(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        let Some(active) = self
            .active_threads
            .get_mut(thread)
            .filter(|active| &active.account_id == account)
        else {
            return false;
        };
        let Some(streams) = active.streams.checked_sub(1) else {
            return false;
        };
        active.streams = streams;
        active.last_activity_at = at;
        if streams == 0 && active.reservations == 0 && !active.awaiting_follow_up {
            self.active_threads.remove(thread);
        }
        true
    }

    pub fn connection_continues(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        let Some(active) = self
            .active_threads
            .get_mut(thread)
            .filter(|active| &active.account_id == account)
        else {
            return false;
        };
        let Some(streams) = active.streams.checked_sub(1) else {
            return false;
        };
        active.streams = streams;
        active.awaiting_follow_up = true;
        active.last_activity_at = at;
        true
    }

    pub fn reset_connections(&mut self, _at: UnixMillis) -> bool {
        let changed = !self.attached_threads.is_empty() || !self.active_threads.is_empty();
        let changed = self.clear_abandoned_reservations() | changed;
        self.attached_threads.clear();
        self.active_threads.retain(|_, active| {
            if !active.awaiting_follow_up {
                return false;
            }
            active.streams = 0;
            active.reservations = 0;
            true
        });
        changed
    }

    pub(super) fn cancel_active_thread(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        if self.active_threads.get(thread).is_some_and(|active| {
            &active.account_id == account && active.streams == 0 && active.reservations == 0
        }) {
            self.active_threads.remove(thread);
            true
        } else {
            false
        }
    }

    pub(super) fn reconcile_active_threads(
        &mut self,
        known: &BTreeSet<AccountId>,
        now: UnixMillis,
    ) -> bool {
        let before = self.active_threads.clone();
        let reservations_changed = self.expire_reservations(now);
        self.active_threads.retain(|_, thread| {
            known.contains(&thread.account_id)
                && (thread.streams > 0
                    || thread.reservations > 0
                    || (thread.awaiting_follow_up
                        && thread
                            .last_activity_at
                            .get()
                            .saturating_add(ABANDONED_FOLLOW_UP_MILLIS)
                            > now.get()))
        });
        reservations_changed | (self.active_threads != before)
    }
}
