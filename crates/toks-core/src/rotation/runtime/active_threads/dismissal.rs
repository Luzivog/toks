use std::collections::BTreeSet;

use super::RotationRuntime;
use crate::rotation::ThreadId;

impl RotationRuntime {
    /// Apply explicit user intent without weakening lifecycle retention for
    /// follow-ups. Live work and attached clients remain authoritative.
    pub(crate) fn dismiss_cancelled_threads(
        &mut self,
        cancelled: &BTreeSet<ThreadId>,
    ) -> BTreeSet<ThreadId> {
        let dismissed = self
            .active_threads
            .iter()
            .filter(|(thread, active)| {
                cancelled.contains(*thread)
                    && active.stream_count() == 0
                    && active.reservations == 0
                    && !self.attached_threads.contains_key(*thread)
            })
            .map(|(thread, _)| thread.clone())
            .collect::<BTreeSet<_>>();
        for thread in &dismissed {
            self.active_threads.remove(thread);
            for account in self.accounts.values_mut() {
                account.grandfathered_threads.remove(thread);
                account.provisional_threads.remove(thread);
                account.thread_usage.remove(thread);
            }
        }
        dismissed
    }
}
