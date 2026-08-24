use std::collections::BTreeSet;

use crate::codex_router::host::GenerationId;

use super::Workers;

impl Workers {
    pub(in crate::codex_router::host::process) fn is_ready(
        &self,
        generation: GenerationId,
    ) -> bool {
        self.slots
            .get(&generation)
            .is_some_and(|worker| worker.ready)
    }

    pub(in crate::codex_router::host::process) fn is_accepting(
        &self,
        generation: GenerationId,
    ) -> bool {
        self.slots
            .get(&generation)
            .is_some_and(|worker| worker.accepting)
    }

    pub(in crate::codex_router::host::process) fn is_draining(
        &self,
        generation: GenerationId,
    ) -> bool {
        self.slots
            .get(&generation)
            .is_some_and(|worker| worker.draining)
    }

    pub(in crate::codex_router::host::process) fn mark_ready(&mut self, generation: GenerationId) {
        if let Some(worker) = self.slots.get_mut(&generation) {
            worker.ready = true;
        }
    }

    pub(in crate::codex_router::host::process) fn mark_not_accepting(
        &mut self,
        generation: GenerationId,
    ) {
        if let Some(worker) = self.slots.get_mut(&generation) {
            worker.accepting = false;
        }
    }

    pub(in crate::codex_router::host::process) fn mark_admissions_paused(
        &mut self,
        generation: GenerationId,
    ) {
        if let Some(worker) = self.slots.get_mut(&generation) {
            worker.accepting = false;
            worker.draining = true;
        }
    }

    pub(in crate::codex_router::host::process) fn mark_accepting(
        &mut self,
        generation: GenerationId,
    ) -> bool {
        let Some(worker) = self.slots.get_mut(&generation) else {
            return false;
        };
        worker.accepting = true;
        worker.draining = false;
        let reconcile_pending = !worker.pending_reconciled;
        worker.pending_reconciled = true;
        reconcile_pending
    }

    pub(in crate::codex_router::host::process) fn disconnect(&mut self, generation: GenerationId) {
        self.slots.remove(&generation);
        self.disconnected.insert(generation);
    }

    pub(in crate::codex_router::host::process) fn is_disconnected(
        &self,
        generation: GenerationId,
    ) -> bool {
        self.disconnected.contains(&generation)
    }

    pub(in crate::codex_router::host::process) fn mark_stopped(
        &mut self,
        generation: GenerationId,
    ) {
        self.stopped.insert(generation);
    }

    pub(in crate::codex_router::host::process) fn mark_stopped_generations(
        &mut self,
        generations: BTreeSet<GenerationId>,
    ) {
        self.stopped.extend(generations);
    }

    pub(in crate::codex_router::host::process) fn is_stopped(
        &self,
        generation: GenerationId,
    ) -> bool {
        self.stopped.contains(&generation)
    }
}
