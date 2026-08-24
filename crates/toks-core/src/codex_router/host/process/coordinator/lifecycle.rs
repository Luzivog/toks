use anyhow::Result;

use crate::codex_router::handoff::Control;
use crate::codex_router::host::{DeployPlan, DeploymentEvent, GenerationStatus};

use super::core::Coordinator;
use super::wait::WaitKey;

const LIVENESS_EVIDENCE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(3);

impl Coordinator {
    pub(super) async fn ensure_active_accepting(&mut self, plan: &DeployPlan) -> Result<bool> {
        if !matches!(plan, DeployPlan::StageTarget { .. }) {
            return Ok(false);
        }
        let active = self
            .deployment
            .snapshot()
            .generations
            .into_iter()
            .find_map(|generation| {
                (generation.status == GenerationStatus::Active).then_some(generation.id)
            });
        let Some(active) = active else {
            return Ok(false);
        };
        if !self.worker_ready(active) {
            return Ok(true);
        }
        if self
            .workers
            .get(&active)
            .is_some_and(|worker| worker.accepting)
        {
            return Ok(false);
        }
        let wait = WaitKey::TargetAccepting(active);
        if !self.waiting_for(wait)
            && self
                .send(
                    active,
                    Control::Activate {
                        generation: active.into(),
                    },
                )
                .await
                .is_ok()
        {
            self.arm_wait(wait);
        }
        Ok(true)
    }

    pub(super) async fn expire_waits(&mut self) -> Result<()> {
        let expired = self
            .deployment_wait
            .take_expired(tokio::time::Instant::now());
        if expired.is_empty() {
            return Ok(());
        }
        let plan = self.current_plan()?;
        if let DeployPlan::PauseAdmissions {
            previous: Some(previous),
            target,
        } = &plan
        {
            if expired.contains(&WaitKey::WorkerReady(*previous))
                && self.previous_is_confirmed_lost(*previous, *target).await
            {
                self.record(DeploymentEvent::Failed {
                    generation: *previous,
                    reason: "previous worker disconnected and remained stopped through the recovery deadline"
                        .into(),
                })?;
                self.stopped_workers.insert(*previous);
                self.record(DeploymentEvent::PreviousPaused { target: *target })?;
                return Ok(());
            }
        }
        let failed_target = match plan {
            DeployPlan::StageTarget { target, .. } | DeployPlan::PauseAdmissions { target, .. }
                if expired.contains(&WaitKey::WorkerReady(target)) =>
            {
                Some(target)
            }
            DeployPlan::StartAccepting { target }
                if expired.contains(&WaitKey::WorkerReady(target))
                    || expired.contains(&WaitKey::TargetAccepting(target)) =>
            {
                Some(target)
            }
            DeployPlan::ResumeAdmissions { .. }
            | DeployPlan::Retire { .. }
            | DeployPlan::Settled { .. }
            | DeployPlan::Unavailable { .. }
            | DeployPlan::StageTarget { .. }
            | DeployPlan::PauseAdmissions { .. }
            | DeployPlan::StartAccepting { .. } => None,
        };
        if let Some(target) = failed_target {
            self.record(DeploymentEvent::Failed {
                generation: target,
                reason: "worker activation acknowledgement timed out".into(),
            })?;
            if self.command_worker("stop", target).await.is_ok() {
                self.stopped_workers.insert(target);
            }
        }
        Ok(())
    }

    async fn previous_is_confirmed_lost(
        &self,
        previous: crate::codex_router::host::GenerationId,
        target: crate::codex_router::host::GenerationId,
    ) -> bool {
        if !self.disconnected_workers.contains(&previous)
            || self.worker_ready(previous)
            || !self.worker_ready(target)
        {
            return false;
        }
        let Ok(Ok(inventory)) =
            tokio::time::timeout(LIVENESS_EVIDENCE_TIMEOUT, (self.worker_inventory)()).await
        else {
            return false;
        };
        inventory
            .get(&previous)
            .is_none_or(|liveness| *liveness == super::worker_unit::Liveness::Stopped)
    }

    pub(super) fn arm_wait(&mut self, key: WaitKey) {
        self.deployment_wait.arm(key, tokio::time::Instant::now());
    }

    pub(super) fn waiting_for(&self, key: WaitKey) -> bool {
        self.deployment_wait.is_armed(key)
    }
}
