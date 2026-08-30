use std::collections::BTreeSet;

use super::{RotationRuntime, RotationSettings, ThreadId};

impl RotationSettings {
    pub fn cancelled_threads(&self) -> &BTreeSet<ThreadId> {
        &self.cancelled_threads
    }

    pub fn waiting_priority(&self) -> &[ThreadId] {
        &self.waiting_priority
    }

    pub fn cancel_thread(&mut self, thread: &ThreadId) -> bool {
        let changed = self.cancelled_threads.insert(thread.clone());
        self.waiting_priority.retain(|queued| queued != thread);
        changed
    }

    pub fn restore_waiting(&mut self, thread: &ThreadId) -> bool {
        if !self.cancelled_threads.remove(thread) {
            return false;
        }
        if !self.waiting_priority.contains(thread) {
            self.waiting_priority.push(thread.clone());
        }
        true
    }

    pub fn reconcile_thread_state(&mut self, runtime: &RotationRuntime) -> bool {
        let waiting = runtime.queued_or_resuming_threads();
        let retained = runtime
            .retained_thread_ids()
            .into_iter()
            .collect::<Vec<_>>();
        self.reconcile_threads(&waiting, &retained)
    }

    pub(crate) fn reconcile_threads(&mut self, waiting: &[ThreadId], active: &[ThreadId]) -> bool {
        let cancelled_before = self.cancelled_threads.clone();
        let priority_before = self.waiting_priority.clone();
        let waiting_set = waiting.iter().cloned().collect::<BTreeSet<_>>();
        let present = waiting_set
            .iter()
            .chain(active)
            .cloned()
            .collect::<BTreeSet<_>>();
        let forgotten = self
            .cancelled_threads
            .iter()
            .filter(|thread| !present.contains(*thread))
            .cloned()
            .collect::<BTreeSet<_>>();
        self.cancelled_threads
            .retain(|thread| present.contains(thread));
        let overrides_before = self.thread_overrides.len();
        self.thread_overrides
            .retain(|thread, _| !forgotten.contains(thread));
        let mut seen = BTreeSet::new();
        self.waiting_priority.retain(|thread| {
            waiting_set.contains(thread)
                && !self.cancelled_threads.contains(thread)
                && seen.insert(thread.clone())
        });
        for thread in waiting {
            if !self.cancelled_threads.contains(thread) && seen.insert(thread.clone()) {
                self.waiting_priority.push(thread.clone());
            }
        }
        cancelled_before != self.cancelled_threads
            || priority_before != self.waiting_priority
            || overrides_before != self.thread_overrides.len()
    }

    pub(super) fn normalize_waiting(&mut self) {
        let mut seen = BTreeSet::new();
        self.waiting_priority.retain(|thread| {
            !self.cancelled_threads.contains(thread) && seen.insert(thread.clone())
        });
    }
}
