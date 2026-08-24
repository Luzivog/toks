use anyhow::Result;

use crate::codex_router::handoff::Control;
use crate::codex_router::host::{DeployPlan, DeploymentEvent, GenerationId};

use super::super::paths::save_state;
use super::core::Coordinator;
use super::wait::WaitKey;

impl Coordinator {
    pub(in crate::codex_router::host::process) fn worker_disconnected(
        &mut self,
        generation: u64,
    ) -> Result<()> {
        self.disconnected_workers
            .insert(GenerationId::from_raw(generation));
        let plan = self.current_plan()?;
        let target = match plan {
            DeployPlan::StageTarget { target, .. }
            | DeployPlan::PauseAdmissions { target, .. }
            | DeployPlan::StartAccepting { target }
                if target.get() == generation =>
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
        generation: u64,
        message: Control,
    ) -> Result<()> {
        match message {
            Control::Ready { generation: found } if found.raw() == generation => {
                let id = GenerationId::from_raw(generation);
                self.deployment_wait.acknowledge(WaitKey::WorkerReady(id));
                if let Some(worker) = self.workers.get_mut(&generation) {
                    worker.ready = true;
                }
            }
            Control::AdmissionsPaused { generation: found } if found.raw() == generation => {
                self.deployment_wait.acknowledge(WaitKey::AdmissionsPaused(
                    GenerationId::from_raw(generation),
                ));
                if let Some(worker) = self.workers.get_mut(&generation) {
                    worker.accepting = false;
                    worker.draining = true;
                }
                let plan = self.current_plan()?;
                if let DeployPlan::PauseAdmissions {
                    previous: Some(previous),
                    target,
                } = plan
                {
                    if previous.get() == generation {
                        self.record(DeploymentEvent::PreviousPaused { target })?;
                    }
                }
            }
            Control::Accepting { generation: found } if found.raw() == generation => {
                let id = GenerationId::from_raw(generation);
                self.deployment_wait
                    .acknowledge(WaitKey::TargetAccepting(id));
                self.deployment_wait
                    .acknowledge(WaitKey::AdmissionsResumed(id));
                let reconcile_pending = if let Some(worker) = self.workers.get_mut(&generation) {
                    worker.accepting = true;
                    worker.draining = false;
                    let reconcile = !worker.pending_reconciled;
                    worker.pending_reconciled = true;
                    reconcile
                } else {
                    false
                };
                let plan = self.current_plan()?;
                match plan {
                    DeployPlan::StartAccepting { target } if target.get() == generation => {
                        self.record(DeploymentEvent::TargetAccepting { target })?;
                    }
                    DeployPlan::ResumeAdmissions {
                        previous,
                        failed_target,
                    } if previous.get() == generation => {
                        self.record(DeploymentEvent::AdmissionsResumed { failed_target })?;
                    }
                    _ => {}
                }
                if reconcile_pending {
                    self.retry_pending(generation).await?;
                }
            }
            Control::ConnectionAck { handoff_id }
                if self
                    .pending
                    .mark_committing(wire_generation(generation), handoff_id) =>
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
                let should_finalize = self
                    .pending
                    .begin_finalizing(wire_generation(generation), handoff_id);
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
                    .acknowledge_finalized(wire_generation(generation), handoff_id);
            }
            Control::ConnectionsObserved {
                generation: found,
                active,
            } if found.raw() == generation => {
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

fn wire_generation(generation: u64) -> crate::codex_router::handoff::GenerationId {
    crate::codex_router::handoff::GenerationId::new(generation)
}
