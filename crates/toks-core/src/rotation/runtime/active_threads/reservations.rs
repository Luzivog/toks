use crate::accounts::AccountId;

use super::{ActiveThread, RotationRuntime, ThreadId, UnixMillis};

const ABANDONED_RESERVATION_MILLIS: i64 = 5 * 60 * 1_000;

impl ActiveThread {
    pub(in crate::rotation::runtime) fn reservation_only(&self) -> bool {
        self.reservations > 0 && self.streams == 0 && !self.awaiting_follow_up
    }
}

impl RotationRuntime {
    pub fn reserve_thread(&mut self, account: &AccountId, thread: &ThreadId, at: UnixMillis) {
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
            active.awaiting_follow_up = false;
        }
        active.reservations = active.reservations.saturating_add(1);
        active.last_activity_at = at;
    }

    pub fn release_reservation(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        let Some(active) = self
            .active_threads
            .get_mut(thread)
            .filter(|active| &active.account_id == account && active.reservations > 0)
        else {
            return false;
        };
        active.reservations -= 1;
        if active.reservations == 0 && active.streams == 0 && !active.awaiting_follow_up {
            self.active_threads.remove(thread);
            self.clear_provisional(account, thread);
        }
        true
    }

    pub(super) fn clear_abandoned_reservations(&mut self) -> bool {
        let abandoned = self
            .active_threads
            .iter()
            .filter(|(_, active)| active.reservation_only())
            .map(|(thread, active)| (thread.clone(), active.account_id.clone()))
            .collect::<Vec<_>>();
        abandoned
            .into_iter()
            .fold(false, |changed, (thread, account)| {
                self.clear_provisional(&account, &thread) | changed
            })
    }

    pub(super) fn expire_reservations(&mut self, now: UnixMillis) -> bool {
        let before = self.active_threads.clone();
        let expired = self
            .active_threads
            .iter()
            .filter(|(_, thread)| {
                thread.reservation_only()
                    && thread
                        .last_activity_at
                        .get()
                        .saturating_add(ABANDONED_RESERVATION_MILLIS)
                        <= now.get()
            })
            .map(|(thread, active)| (thread.clone(), active.account_id.clone()))
            .collect::<Vec<_>>();
        let mut changed = false;
        for (thread, account) in expired {
            changed |= self.clear_provisional(&account, &thread);
        }
        self.active_threads.retain(|_, thread| {
            if thread.reservations > 0
                && thread
                    .last_activity_at
                    .get()
                    .saturating_add(ABANDONED_RESERVATION_MILLIS)
                    <= now.get()
            {
                thread.reservations = 0;
            }
            true
        });
        changed | (self.active_threads != before)
    }

    fn clear_provisional(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        self.accounts.get_mut(account).is_some_and(|state| {
            if !state.provisional_threads.remove(thread) {
                return false;
            }
            state.grandfathered_threads.remove(thread);
            true
        })
    }
}
