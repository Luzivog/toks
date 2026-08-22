use std::collections::BTreeSet;

use super::{RotationSettings, ThreadId};

impl RotationSettings {
    pub fn cancelled_threads(&self) -> &BTreeSet<ThreadId> {
        &self.cancelled_threads
    }

    pub fn waiting_priority(&self) -> &[ThreadId] {
        &self.waiting_priority
    }

    pub fn cancel_waiting(&mut self, thread: &ThreadId) -> bool {
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

    pub fn move_waiting_to(&mut self, thread: &ThreadId, index: usize) -> bool {
        let Some(from) = self
            .waiting_priority
            .iter()
            .position(|queued| queued == thread)
        else {
            return false;
        };
        let destination = index.min(self.waiting_priority.len().saturating_sub(1));
        if from == destination {
            return false;
        }
        let thread = self.waiting_priority.remove(from);
        self.waiting_priority.insert(destination, thread);
        true
    }

    pub fn reconcile_waiting(&mut self, waiting: &[ThreadId]) -> bool {
        let before = (
            self.cancelled_threads.clone(),
            self.waiting_priority.clone(),
        );
        let known: BTreeSet<_> = waiting.iter().cloned().collect();
        self.cancelled_threads
            .retain(|thread| known.contains(thread));
        let mut seen = BTreeSet::new();
        self.waiting_priority.retain(|thread| {
            known.contains(thread)
                && !self.cancelled_threads.contains(thread)
                && seen.insert(thread.clone())
        });
        for thread in waiting {
            if !self.cancelled_threads.contains(thread) && seen.insert(thread.clone()) {
                self.waiting_priority.push(thread.clone());
            }
        }
        before
            != (
                self.cancelled_threads.clone(),
                self.waiting_priority.clone(),
            )
    }

    pub(super) fn normalize_waiting(&mut self) {
        let mut seen = BTreeSet::new();
        self.waiting_priority.retain(|thread| {
            !self.cancelled_threads.contains(thread) && seen.insert(thread.clone())
        });
    }
}
