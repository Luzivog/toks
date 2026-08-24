use tokio::time::Instant;

use crate::codex_router::handoff::Control;

use super::core::Coordinator;
use super::pending::{AbandonedStage, HANDOFF_SETTLE_TIMEOUT};

/// A reap pass stops notifying workers after this long so a stalled worker
/// cannot hold up the reconcile tick.
const REAP_NOTIFY_BUDGET: std::time::Duration = std::time::Duration::from_millis(100);

impl Coordinator {
    /// Reclaims handoff slots whose client gave up before delivery settled.
    ///
    /// Every abandoned handoff also releases the client descriptor the
    /// coordinator was holding for a redelivery that will never happen. Any
    /// handoff the worker may have seen additionally gets one best-effort
    /// `ConnectionFinalized`, because the worker can be holding state for it —
    /// a parked descriptor that will never be committed, or an idempotency
    /// tombstone for a commit whose acknowledgement was lost — that would
    /// otherwise sit there for the rest of the coordinator epoch. A lost send
    /// only postpones that reclamation to the next epoch.
    pub(super) async fn reap_stale_handoffs(&mut self, now: Instant) {
        let abandoned = self.pending.reap_expired(now, HANDOFF_SETTLE_TIMEOUT);
        let deadline = Instant::now() + REAP_NOTIFY_BUDGET;
        for handoff in abandoned {
            eprintln!(
                "router abandoned handoff {}/{} for generation {} stuck in {}",
                handoff.id.coordinator_epoch(),
                handoff.id.sequence(),
                handoff.generation.raw(),
                handoff.stage.name(),
            );
            // A queued handoff was never offered to a worker; nothing to clean.
            if handoff.stage == AbandonedStage::Queued {
                continue;
            }
            let Some(generation) = self.host_generation(handoff.generation.raw()) else {
                continue;
            };
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                continue;
            }
            let _ = tokio::time::timeout(
                remaining,
                self.send(
                    generation,
                    Control::ConnectionFinalized {
                        handoff_id: handoff.id,
                    },
                ),
            )
            .await;
        }
    }
}
