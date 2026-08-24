use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::time::Duration;

use crate::codex_router::host::{
    GenerationId, GenerationStatus, COORDINATOR_PRE_SIGNAL_OPERATION_TIMEOUT,
};

use super::core::Coordinator;
use super::wait::WaitKey;

impl Coordinator {
    pub(super) async fn command_worker(
        &self,
        action: &'static str,
        generation: GenerationId,
    ) -> Result<()> {
        self.command_workers(action, vec![generation]).await
    }

    pub(super) async fn command_workers(
        &self,
        action: &'static str,
        generations: Vec<GenerationId>,
    ) -> Result<()> {
        (self.worker_command)(action, generations).await
    }

    pub(super) async fn reconcile_worker_units(&mut self) -> Result<bool> {
        self.reconcile_worker_units_with_timeout(COORDINATOR_PRE_SIGNAL_OPERATION_TIMEOUT)
            .await
    }

    #[cfg(test)]
    pub(super) async fn reconcile_worker_units_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<bool> {
        tokio::time::timeout(timeout, self.reconcile_worker_units_once())
            .await
            .context("worker unit reconciliation timed out")?
    }

    #[cfg(not(test))]
    async fn reconcile_worker_units_with_timeout(&mut self, timeout: Duration) -> Result<bool> {
        tokio::time::timeout(timeout, self.reconcile_worker_units_once())
            .await
            .context("worker unit reconciliation timed out")?
    }

    async fn reconcile_worker_units_once(&mut self) -> Result<bool> {
        let generations = self.deployment.snapshot().generations;
        let needs_inventory = generations.iter().any(|generation| {
            is_terminal(generation.status) && !self.stopped_workers.contains(&generation.id)
                || is_live(generation.status)
                    && !self.worker_ready(generation.id)
                    && !self
                        .deployment_wait
                        .is_armed(WaitKey::WorkerReady(generation.id))
        });
        if !needs_inventory {
            return Ok(false);
        }
        let inventory = (self.worker_inventory)().await?;
        let terminal = generations
            .iter()
            .filter(|generation| {
                is_terminal(generation.status) && !self.stopped_workers.contains(&generation.id)
            })
            .map(|generation| generation.id)
            .collect::<BTreeSet<_>>();
        let loaded_terminal = terminal
            .iter()
            .filter(|generation| inventory.contains_key(generation))
            .copied()
            .collect::<Vec<_>>();
        if !loaded_terminal.is_empty() {
            self.command_workers("stop", loaded_terminal).await?;
        }
        self.stopped_workers.extend(terminal);

        let starts = generations
            .iter()
            .filter(|generation| {
                is_live(generation.status)
                    && !self.worker_ready(generation.id)
                    && !self
                        .deployment_wait
                        .is_armed(WaitKey::WorkerReady(generation.id))
            })
            .map(|generation| generation.id)
            .collect::<Vec<_>>();
        if !starts.is_empty() {
            let _ = self.command_workers("start", starts.clone()).await;
            let now = tokio::time::Instant::now();
            for generation in starts {
                self.deployment_wait
                    .arm(WaitKey::WorkerReady(generation), now);
            }
        }
        Ok(false)
    }
}

fn is_terminal(status: GenerationStatus) -> bool {
    matches!(status, GenerationStatus::Failed | GenerationStatus::Retired)
}

fn is_live(status: GenerationStatus) -> bool {
    matches!(
        status,
        GenerationStatus::Staged | GenerationStatus::Active | GenerationStatus::Draining
    )
}
