use std::time::Duration;

use tokio::time::Instant;

use crate::codex_router::handoff::{GenerationId as WireGenerationId, HandoffId};

use super::Pending;

/// A handoff that has not settled within this window is abandoned.
///
/// Delivery settles in well under a millisecond against a healthy worker, and
/// the longest legitimate stall is a client queued across a worker activation.
/// Anything older belongs to a client that vanished mid-handshake: without this
/// its slot and its descriptor are never reclaimed, and once the in-flight cap
/// fills the coordinator silently stops accepting clients for good.
pub(crate) const HANDOFF_SETTLE_TIMEOUT: Duration = Duration::from_secs(15);

/// One handoff the coordinator gave up on, reported so an unhealthy worker
/// shows up in the log instead of as an unexplained capacity leak.
pub(crate) struct AbandonedHandoff {
    pub(crate) generation: WireGenerationId,
    pub(crate) id: HandoffId,
    pub(crate) stage: &'static str,
}

#[derive(Clone, Copy, Eq, PartialEq)]
pub(super) enum DeliveryPhase {
    Queued,
    Preparing,
    Committing,
}

impl DeliveryPhase {
    fn stage(self) -> &'static str {
        match self {
            Self::Queued => "queued",
            Self::Preparing => "preparing",
            Self::Committing => "committing",
        }
    }
}

impl Pending {
    /// Drops handoffs that never settled, closing their client descriptors, and
    /// returns them so the caller can report what was abandoned.
    pub(crate) fn reap_expired(
        &mut self,
        now: Instant,
        timeout: Duration,
    ) -> Vec<AbandonedHandoff> {
        let mut abandoned = Vec::new();
        self.connections.retain(|&(epoch, sequence), pending| {
            if now.saturating_duration_since(pending.armed) < timeout {
                return true;
            }
            abandoned.push(AbandonedHandoff {
                generation: pending.generation,
                id: HandoffId::new(epoch, sequence),
                stage: pending.phase.stage(),
            });
            false
        });
        self.finalizing
            .retain(|&(generation, epoch, sequence), armed| {
                if now.saturating_duration_since(*armed) < timeout {
                    return true;
                }
                abandoned.push(AbandonedHandoff {
                    generation: WireGenerationId::new(generation),
                    id: HandoffId::new(epoch, sequence),
                    stage: "finalizing",
                });
                false
            });
        abandoned
    }
}
