use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use crate::codex_router::handoff::WorkerInstanceId;
use crate::codex_router::host::GenerationId;

use super::WorkerSlot;
use crate::codex_router::host::process::channel::AsyncChannel;

mod state;

pub(in crate::codex_router::host::process) struct Workers {
    slots: HashMap<GenerationId, WorkerSlot>,
    stopped: BTreeSet<GenerationId>,
    disconnected: BTreeSet<GenerationId>,
    next_registration: u64,
}

impl Workers {
    pub(in crate::codex_router::host::process) fn new(
        disconnected: BTreeSet<GenerationId>,
    ) -> Self {
        Self {
            slots: HashMap::new(),
            stopped: BTreeSet::new(),
            disconnected,
            next_registration: 1,
        }
    }

    pub(in crate::codex_router::host::process) fn register(
        &mut self,
        generation: GenerationId,
        instance: WorkerInstanceId,
        channel: Arc<AsyncChannel>,
    ) -> u64 {
        let registration = self.next_registration;
        self.next_registration = self.next_registration.saturating_add(1);
        self.disconnected.remove(&generation);
        self.slots.insert(
            generation,
            WorkerSlot {
                registration,
                instance,
                ready: false,
                accepting: false,
                draining: false,
                pending_reconciled: false,
                channel,
            },
        );
        registration
    }

    #[cfg(test)]
    pub(in crate::codex_router::host::process) fn replace(
        &mut self,
        generation: GenerationId,
        worker: WorkerSlot,
    ) {
        self.slots.insert(generation, worker);
    }

    #[cfg(test)]
    pub(in crate::codex_router::host::process) fn remove_registered(
        &mut self,
        generation: GenerationId,
    ) -> Option<WorkerSlot> {
        self.slots.remove(&generation)
    }

    #[cfg(test)]
    pub(in crate::codex_router::host::process) fn clear_registered(&mut self) {
        self.slots.clear();
    }

    pub(in crate::codex_router::host::process) fn contains(
        &self,
        generation: GenerationId,
    ) -> bool {
        self.slots.contains_key(&generation)
    }

    pub(in crate::codex_router::host::process) fn is_current(
        &self,
        generation: GenerationId,
        registration: u64,
    ) -> bool {
        self.slots
            .get(&generation)
            .is_some_and(|worker| worker.registration == registration)
    }

    pub(in crate::codex_router::host::process) fn channel_for(
        &self,
        generation: GenerationId,
    ) -> Option<Arc<AsyncChannel>> {
        self.slots
            .get(&generation)
            .map(|worker| worker.channel.clone())
    }

    pub(in crate::codex_router::host::process) fn ready_generations(&self) -> Vec<GenerationId> {
        self.slots
            .iter()
            .filter_map(|(generation, worker)| worker.ready.then_some(*generation))
            .collect()
    }

    pub(in crate::codex_router::host::process) fn ready_instance(
        &self,
        generation: GenerationId,
    ) -> Option<u64> {
        let worker = self.slots.get(&generation)?;
        worker.ready.then_some(worker.instance.raw())
    }
}
