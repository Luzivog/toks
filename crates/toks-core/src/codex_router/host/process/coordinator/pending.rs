use std::collections::BTreeMap;

use tokio::time::Instant;

use crate::codex_router::handoff::{GenerationId as WireGenerationId, HandoffId};

mod reap;
use reap::DeliveryPhase;
pub(super) use reap::{AbandonedStage, HANDOFF_SETTLE_TIMEOUT};

// Once this many descriptors are in flight, the systemd socket backlog buffers
// new clients until a worker completes a handoff.
const MAX_IN_FLIGHT_HANDOFFS: usize = 256;

pub(super) struct PendingConnection {
    pub(super) generation: WireGenerationId,
    pub(super) stream: tokio::net::TcpStream,
    phase: DeliveryPhase,
    armed: Instant,
}

pub(super) struct Pending {
    epoch: u64,
    sequence: u64,
    capacity: usize,
    connections: BTreeMap<(u64, u64), PendingConnection>,
    finalizing: BTreeMap<(u64, u64, u64), Instant>,
}

impl Pending {
    pub(super) fn new() -> anyhow::Result<Self> {
        let mut bytes = [0_u8; 8];
        getrandom::fill(&mut bytes)?;
        Ok(Self {
            epoch: u64::from_ne_bytes(bytes),
            sequence: 1,
            capacity: MAX_IN_FLIGHT_HANDOFFS,
            connections: BTreeMap::new(),
            finalizing: BTreeMap::new(),
        })
    }

    #[cfg(test)]
    pub(super) fn with_capacity(capacity: usize) -> anyhow::Result<Self> {
        let mut pending = Self::new()?;
        pending.capacity = capacity;
        Ok(pending)
    }

    pub(super) fn insert(
        &mut self,
        generation: WireGenerationId,
        stream: tokio::net::TcpStream,
    ) -> anyhow::Result<HandoffId> {
        anyhow::ensure!(self.has_capacity(), "router handoff capacity exhausted");
        let id = HandoffId::new(self.epoch, self.sequence);
        self.sequence = self
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow::anyhow!("router handoff sequence exhausted"))?;
        self.connections.insert(
            key(id),
            PendingConnection {
                generation,
                stream,
                phase: DeliveryPhase::Queued,
                armed: Instant::now(),
            },
        );
        Ok(id)
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(super) fn delivery(
        &self,
        generation: WireGenerationId,
        id: HandoffId,
    ) -> Option<(&tokio::net::TcpStream, bool)> {
        let pending = self.connections.get(&key(id))?;
        (pending.generation == generation)
            .then_some((&pending.stream, pending.phase != DeliveryPhase::Queued))
    }

    pub(super) fn mark_preparing(&mut self, generation: WireGenerationId, id: HandoffId) -> bool {
        self.transition(generation, id, DeliveryPhase::Preparing)
    }

    pub(super) fn mark_committing(&mut self, generation: WireGenerationId, id: HandoffId) -> bool {
        self.transition(generation, id, DeliveryPhase::Committing)
    }

    pub(super) fn remove(&mut self, generation: WireGenerationId, id: HandoffId) -> bool {
        if self
            .connections
            .get(&key(id))
            .is_none_or(|pending| pending.generation != generation)
        {
            return false;
        }
        self.connections.remove(&key(id));
        true
    }

    pub(super) fn begin_finalizing(&mut self, generation: WireGenerationId, id: HandoffId) -> bool {
        let finalization = finalization_key(generation, id);
        if self.finalizing.contains_key(&finalization) {
            return true;
        }
        if !self.remove(generation, id) {
            return false;
        }
        self.finalizing.insert(finalization, Instant::now());
        true
    }

    pub(super) fn acknowledge_finalized(
        &mut self,
        generation: WireGenerationId,
        id: HandoffId,
    ) -> bool {
        self.finalizing
            .remove(&finalization_key(generation, id))
            .is_some()
    }

    pub(super) fn matching(
        &self,
        generation: WireGenerationId,
    ) -> impl Iterator<Item = (HandoffId, &PendingConnection)> {
        self.connections
            .iter()
            .filter_map(move |(&(epoch, sequence), pending)| {
                (pending.generation == generation)
                    .then_some((HandoffId::new(epoch, sequence), pending))
            })
    }

    pub(super) fn finalizing(
        &self,
        generation: WireGenerationId,
    ) -> impl Iterator<Item = HandoffId> + '_ {
        self.finalizing
            .keys()
            .filter_map(move |&(found, epoch, sequence)| {
                (found == generation.raw()).then_some(HandoffId::new(epoch, sequence))
            })
    }

    pub(super) fn is_empty(&self) -> bool {
        self.connections.is_empty() && self.finalizing.is_empty()
    }

    pub(super) fn has_capacity(&self) -> bool {
        self.connections.len() + self.finalizing.len() < self.capacity
    }

    pub(super) fn has_generation(&self, generation: WireGenerationId) -> bool {
        self.connections
            .values()
            .any(|pending| pending.generation == generation)
            || self
                .finalizing
                .keys()
                .any(|&(found, _, _)| found == generation.raw())
    }

    fn transition(
        &mut self,
        generation: WireGenerationId,
        id: HandoffId,
        phase: DeliveryPhase,
    ) -> bool {
        let Some(pending) = self.connections.get_mut(&key(id)) else {
            return false;
        };
        if pending.generation != generation {
            return false;
        }
        pending.phase = phase;
        true
    }
}

fn key(id: HandoffId) -> (u64, u64) {
    (id.coordinator_epoch(), id.sequence())
}

fn finalization_key(generation: WireGenerationId, id: HandoffId) -> (u64, u64, u64) {
    (generation.raw(), id.coordinator_epoch(), id.sequence())
}
