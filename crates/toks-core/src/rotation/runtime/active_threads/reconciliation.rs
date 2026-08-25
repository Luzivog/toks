use std::collections::BTreeSet;

use crate::accounts::AccountId;

use super::{RotationRuntime, UnixMillis, ABANDONED_FOLLOW_UP_MILLIS};

impl RotationRuntime {
    pub(in crate::rotation::runtime) fn reconcile_active_threads(
        &mut self,
        known: &BTreeSet<AccountId>,
        now: UnixMillis,
    ) -> bool {
        let before = self.active_threads.clone();
        let reservations_changed = self.expire_reservations(known, now);
        let attached = self
            .attached_threads
            .iter()
            .filter(|(_, attachment)| attachment.connections() > 0)
            .map(|(thread, _)| thread.clone())
            .collect::<BTreeSet<_>>();
        self.active_threads.retain(|thread_id, thread| {
            thread.stream_count() > 0
                || thread.reservations > 0
                || attached.contains(thread_id)
                || (thread.awaiting_follow_up
                    && (!known.contains(&thread.account_id)
                        || thread
                            .last_activity_at
                            .get()
                            .saturating_add(ABANDONED_FOLLOW_UP_MILLIS)
                            > now.get()))
        });
        reservations_changed | (self.active_threads != before)
    }
}
