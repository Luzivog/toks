use anyhow::Result;

use crate::codex_router::handoff::Control;
use crate::codex_router::host::{DeployPlan, DeploymentEvent};

use super::core::Coordinator;
use super::wait::WaitKey;
use crate::codex_router::host::process::paths::save_state;

impl Coordinator {
    pub(in crate::codex_router::host::process) async fn advance(&mut self) -> Result<()> {
        loop {
            let plan = self.plan_for_advance()?;
            save_state(&self.paths.state, &self.deployment)?;
            if let DeployPlan::Retire { generation } = plan {
                self.command_worker("stop", generation).await?;
                self.workers.mark_stopped(generation);
                self.record(DeploymentEvent::Retired { generation })?;
                continue;
            }
            if let DeployPlan::StageTarget { target, ref build } = plan {
                if let Err(error) = self.paths.stage(target, build) {
                    self.record(DeploymentEvent::Failed {
                        generation: target,
                        reason: error.to_string(),
                    })?;
                    continue;
                }
            }
            if self.reconcile_worker_units().await? {
                continue;
            }
            if self.ensure_active_accepting(&plan).await? {
                return Ok(());
            }
            match plan {
                DeployPlan::StageTarget { target, .. } => {
                    if !self.workers.is_ready(target) {
                        return Ok(());
                    }
                    self.record(DeploymentEvent::Prepared { target })?;
                }
                DeployPlan::PauseAdmissions { previous, target } => {
                    if !self.workers.is_ready(target) {
                        return Ok(());
                    }
                    let Some(previous) = previous else {
                        self.record(DeploymentEvent::PreviousPaused { target })?;
                        continue;
                    };
                    let previous_failed =
                        self.deployment
                            .snapshot()
                            .generations
                            .iter()
                            .any(|generation| {
                                generation.id == previous
                                    && generation.status
                                        == crate::codex_router::host::GenerationStatus::Failed
                            });
                    if previous_failed {
                        self.record(DeploymentEvent::PreviousPaused { target })?;
                        continue;
                    }
                    if !self.workers.is_ready(previous) {
                        return Ok(());
                    }
                    if self.pending.has_generation(previous.into()) {
                        self.deployment_wait
                            .acknowledge(WaitKey::AdmissionsPaused(previous));
                        return Ok(());
                    }
                    let wait = WaitKey::AdmissionsPaused(previous);
                    if self.waiting_for(wait) {
                        return Ok(());
                    }
                    self.workers.mark_not_accepting(previous);
                    let control = Control::Drain {
                        generation: previous.into(),
                    };
                    if self.send(previous, control).await.is_ok() {
                        self.arm_wait(wait);
                    }
                    return Ok(());
                }
                DeployPlan::StartAccepting { target } => {
                    if !self.workers.is_ready(target) {
                        return Ok(());
                    }
                    let wait = WaitKey::TargetAccepting(target);
                    if self.waiting_for(wait) {
                        return Ok(());
                    }
                    let control = Control::Activate {
                        generation: target.into(),
                    };
                    if self.send(target, control).await.is_ok() {
                        self.arm_wait(wait);
                    }
                    return Ok(());
                }
                DeployPlan::ResumeAdmissions {
                    previous,
                    failed_target: _,
                } => {
                    if !self.workers.is_ready(previous) {
                        return Ok(());
                    }
                    let wait = WaitKey::AdmissionsResumed(previous);
                    if self.waiting_for(wait) {
                        return Ok(());
                    }
                    let control = Control::Activate {
                        generation: previous.into(),
                    };
                    if self.send(previous, control).await.is_ok() {
                        self.arm_wait(wait);
                    }
                    return Ok(());
                }
                DeployPlan::Retire { .. } => unreachable!("handled before unit reconciliation"),
                DeployPlan::Settled { active } => {
                    self.active = active;
                    if active.is_some_and(|active| !self.workers.is_ready(active)) {
                        return Ok(());
                    }
                    self.reconcile_workers().await?;
                    return Ok(());
                }
                DeployPlan::Unavailable {
                    failed_target: _,
                    active,
                } => {
                    self.active = active;
                    if active.is_some_and(|active| !self.workers.is_ready(active)) {
                        return Ok(());
                    }
                    self.reconcile_workers().await?;
                    return Ok(());
                }
            }
        }
    }
}
