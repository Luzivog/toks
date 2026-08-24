use anyhow::{Context, Result};

use crate::codex_router::handoff::{Control, HandoffId};
use crate::codex_router::host::{GenerationId, GenerationStatus};

use super::core::Coordinator;
use super::wait::WaitKey;

const PENDING_RETRY_BUDGET: std::time::Duration = std::time::Duration::from_millis(100);

enum Retry {
    Connection(GenerationId, HandoffId),
    Finalization(GenerationId, HandoffId),
}

impl Coordinator {
    pub(super) async fn reconcile_workers(&mut self) -> Result<()> {
        for generation in self.deployment.snapshot().generations {
            if !self.workers.is_ready(generation.id) {
                continue;
            }
            let accepting = self.workers.is_accepting(generation.id);
            let draining = self.workers.is_draining(generation.id);
            let (control, wait) = match generation.status {
                GenerationStatus::Active if !accepting => (
                    Some(Control::Activate {
                        generation: generation.id.into(),
                    }),
                    Some(WaitKey::TargetAccepting(generation.id)),
                ),
                GenerationStatus::Draining if !draining => (
                    Some(Control::Drain {
                        generation: generation.id.into(),
                    }),
                    Some(WaitKey::AdmissionsPaused(generation.id)),
                ),
                GenerationStatus::Active => (None, None),
                GenerationStatus::Draining => (None, None),
                GenerationStatus::Staged | GenerationStatus::Retired | GenerationStatus::Failed => {
                    (None, None)
                }
            };
            if let (Some(control), Some(wait)) = (control, wait) {
                if self.deployment_wait.is_armed(wait) {
                    continue;
                }
                if self.send(generation.id, control).await.is_ok() {
                    self.deployment_wait.arm(wait, tokio::time::Instant::now());
                }
            }
        }
        Ok(())
    }

    pub(super) async fn retry_pending(&mut self, generation: GenerationId) -> Result<()> {
        let mut deliveries = self
            .pending
            .matching(generation.into())
            .map(|(id, _)| Retry::Connection(generation, id))
            .collect::<Vec<_>>();
        deliveries.extend(
            self.pending
                .finalizing(generation.into())
                .map(|id| Retry::Finalization(generation, id)),
        );
        self.retry_deliveries(deliveries).await;
        Ok(())
    }

    pub(super) async fn retry_all_pending(&mut self) {
        let generations = self.workers.ready_generations();
        let mut deliveries = Vec::new();
        for generation in generations {
            deliveries.extend(
                self.pending
                    .matching(generation.into())
                    .map(|(id, _)| Retry::Connection(generation, id)),
            );
            deliveries.extend(
                self.pending
                    .finalizing(generation.into())
                    .map(|id| Retry::Finalization(generation, id)),
            );
        }
        self.retry_deliveries(deliveries).await;
    }

    async fn retry_deliveries(&mut self, mut deliveries: Vec<Retry>) {
        if deliveries.is_empty() {
            return;
        }
        let start = self.retry_cursor % deliveries.len();
        deliveries.rotate_left(start);
        let deadline = tokio::time::Instant::now() + PENDING_RETRY_BUDGET;
        let mut attempted = 0;
        for delivery in deliveries {
            let now = tokio::time::Instant::now();
            if now >= deadline {
                break;
            }
            let result = match delivery {
                Retry::Connection(generation, id) => {
                    tokio::time::timeout(deadline - now, self.send_pending(generation, id)).await
                }
                Retry::Finalization(generation, handoff_id) => {
                    let Some(generation) = self.host_generation(generation) else {
                        continue;
                    };
                    tokio::time::timeout(
                        deadline - now,
                        self.send(generation, Control::ConnectionFinalized { handoff_id }),
                    )
                    .await
                }
            };
            let _ = result;
            attempted += 1;
        }
        self.retry_cursor = start.saturating_add(attempted);
    }

    pub(super) async fn send(&self, generation: GenerationId, control: Control) -> Result<()> {
        let channel = self
            .workers
            .channel_for(generation)
            .context("worker disconnected")?;
        tokio::time::timeout(
            super::core::CONTROL_SEND_TIMEOUT,
            channel.send_control(&control),
        )
        .await
        .map_err(|_| anyhow::anyhow!("worker control send timed out"))??;
        Ok(())
    }

    pub(super) fn host_generation(&self, generation: GenerationId) -> Option<GenerationId> {
        self.deployment
            .snapshot()
            .generations
            .into_iter()
            .find_map(|found| (found.id == generation).then_some(found.id))
    }
}
