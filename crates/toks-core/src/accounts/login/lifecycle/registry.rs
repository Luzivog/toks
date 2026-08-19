use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use super::{LoginKey, LoginOutcome, Tracking};

#[derive(Debug, Clone)]
struct Entry {
    generation: u64,
    outcome: LoginOutcome,
    cancelled: Arc<AtomicBool>,
}

#[derive(Default)]
pub(super) struct Registry {
    entries: BTreeMap<LoginKey, Entry>,
    next_generation: u64,
}

impl Registry {
    pub(super) fn start(&mut self, key: LoginKey) -> Tracking {
        if let Some(previous) = self.entries.get(&key) {
            previous.cancelled.store(true, Ordering::Release);
        }
        self.next_generation = self.next_generation.wrapping_add(1);
        let generation = self.next_generation;
        let cancelled = Arc::new(AtomicBool::new(false));
        self.entries.insert(
            key,
            Entry {
                generation,
                outcome: LoginOutcome::Pending,
                cancelled: cancelled.clone(),
            },
        );
        Tracking {
            generation,
            cancelled,
        }
    }

    pub(super) fn finish(&mut self, key: &LoginKey, generation: u64, outcome: LoginOutcome) {
        if let Some(entry) = self.entries.get_mut(key) {
            if entry.generation == generation && entry.outcome == LoginOutcome::Pending {
                entry.outcome = outcome;
            }
        }
    }

    pub(super) fn cancel(&mut self, key: &LoginKey) -> bool {
        let Some(entry) = self.entries.get_mut(key) else {
            return false;
        };
        if entry.outcome == LoginOutcome::Pending {
            entry.outcome = LoginOutcome::Cancelled;
        }
        entry.cancelled.store(true, Ordering::Release);
        true
    }

    pub(super) fn outcome(&self, key: &LoginKey) -> Option<LoginOutcome> {
        self.entries.get(key).map(|entry| entry.outcome)
    }

    pub(super) fn is_pending(&self, key: &LoginKey, generation: u64) -> bool {
        self.entries.get(key).is_some_and(|entry| {
            entry.generation == generation && entry.outcome == LoginOutcome::Pending
        })
    }
}
