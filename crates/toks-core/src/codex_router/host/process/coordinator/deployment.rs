use anyhow::Result;

use crate::codex_router::handoff::Control;
use crate::codex_router::host::{DeployPlan, DeploymentEvent, GenerationId};

use super::super::paths::save_state;
use super::core::Coordinator;
use super::wait::WaitKey;

impl Coordinator {
    pub(in crate::codex_router::host::process) fn worker_disconnected(
        &mut self,
        generation: GenerationId,
    ) -> Result<()> {
        self.workers.disconnect(generation);
        let plan = self.current_plan()?;
        let target = match plan {
            DeployPlan::StageTarget { target, .. }
            | DeployPlan::PauseAdmissions { target, .. }
            | DeployPlan::StartAccepting { target }
                if target == generation =>
            {
                target
            }
            _ => return Ok(()),
        };
        self.record(DeploymentEvent::Failed {
            generation: target,
            reason: "worker disconnected during activation".into(),
        })
    }

    pub(in crate::codex_router::host::process) async fn handle_message(
        &mut self,
        generation: GenerationId,
        message: Control,
    ) -> Result<()> {
        let wire_generation = generation.into();
        match message {
            Control::Ready { generation: found } if found == wire_generation => {
                self.deployment_wait
                    .acknowledge(WaitKey::WorkerReady(generation));
                self.workers.mark_ready(generation);
            }
            Control::AdmissionsPaused { generation: found } if found == wire_generation => {
                self.deployment_wait
                    .acknowledge(WaitKey::AdmissionsPaused(generation));
                self.workers.mark_admissions_paused(generation);
                let plan = self.current_plan()?;
                if let DeployPlan::PauseAdmissions {
                    previous: Some(previous),
                    target,
                } = plan
                {
                    if previous == generation {
                        self.record(DeploymentEvent::PreviousPaused { target })?;
                    }
                }
            }
            Control::Accepting { generation: found } if found == wire_generation => {
                self.deployment_wait
                    .acknowledge(WaitKey::TargetAccepting(generation));
                self.deployment_wait
                    .acknowledge(WaitKey::AdmissionsResumed(generation));
                let reconcile_pending = self.workers.mark_accepting(generation);
                let plan = self.current_plan()?;
                match plan {
                    DeployPlan::StartAccepting { target } if target == generation => {
                        self.record(DeploymentEvent::TargetAccepting { target })?;
                    }
                    DeployPlan::ResumeAdmissions {
                        previous,
                        failed_target,
                    } if previous == generation => {
                        self.record(DeploymentEvent::AdmissionsResumed { failed_target })?;
                    }
                    _ => {}
                }
                if reconcile_pending {
                    self.retry_pending(generation).await?;
                }
            }
            Control::ConnectionAck { handoff_id }
                if self.pending.mark_committing(wire_generation, handoff_id) =>
            {
                // An acknowledgement can outlive its generation's entry in the
                // deployment state, so a missing one is retirement, not a bug.
                if let Some(worker_generation) = self.host_generation(generation) {
                    let _ = self
                        .send(
                            worker_generation,
                            Control::ConnectionCommitted { handoff_id },
                        )
                        .await;
                }
            }
            Control::ConnectionCommitAck { handoff_id } => {
                let should_finalize = self.pending.begin_finalizing(wire_generation, handoff_id);
                if let (true, Some(worker_generation)) =
                    (should_finalize, self.host_generation(generation))
                {
                    let _ = self
                        .send(
                            worker_generation,
                            Control::ConnectionFinalized { handoff_id },
                        )
                        .await;
                }
            }
            Control::ConnectionFinalizedAck { handoff_id } => {
                self.pending
                    .acknowledge_finalized(wire_generation, handoff_id);
            }
            Control::ConnectionsObserved {
                generation: found,
                active,
            } if found == wire_generation => {
                if let Some(id) = self.host_generation(generation) {
                    if self
                        .deployment
                        .reconcile(DeploymentEvent::ConnectionsObserved {
                            generation: id,
                            active,
                        })
                        .is_ok()
                    {
                        save_state(&self.paths.state, &self.deployment)?;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub(super) fn record(&mut self, event: DeploymentEvent) -> Result<()> {
        self.deployment.reconcile(event)?;
        save_state(&self.paths.state, &self.deployment)
    }
}
