use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::model::{
    ActivationPhase, ActivationSnapshot, BuildId, DeployError, DeployPlan, DeploymentEvent,
    DeploymentSnapshot, GenerationId, GenerationSnapshot, GenerationStatus,
};

const STATE_VERSION: u8 = 1;

mod planning;
mod retry;
mod transitions;
mod validation;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Generation {
    build: BuildId,
    status: GenerationStatus,
    active_connections: Option<u64>,
    failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Activation {
    target: GenerationId,
    previous: Option<GenerationId>,
    phase: ActivationPhase,
    failure: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RetryReceipt {
    build: BuildId,
    generation: GenerationId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeploymentState {
    version: u8,
    next_generation: u64,
    generations: BTreeMap<GenerationId, Generation>,
    activation: Option<Activation>,
    #[serde(default)]
    last_rollback: Option<GenerationId>,
    #[serde(default)]
    retry_receipts: BTreeMap<super::RetryId, RetryReceipt>,
}

impl Default for DeploymentState {
    fn default() -> Self {
        Self {
            version: STATE_VERSION,
            next_generation: 1,
            generations: BTreeMap::new(),
            activation: None,
            last_rollback: None,
            retry_receipts: BTreeMap::new(),
        }
    }
}

impl DeploymentState {
    pub fn reserve_generation_ids_after(&mut self, existing: u64) -> Result<(), DeployError> {
        self.validate()?;
        self.next_generation = self.next_generation.max(
            existing
                .checked_add(1)
                .ok_or(DeployError::GenerationIdsExhausted)?,
        );
        Ok(())
    }

    /// Records deployment intent before returning the next idempotent host action.
    pub fn plan_deploy(&mut self, build: BuildId) -> Result<DeployPlan, DeployError> {
        self.validate()?;
        if let Some((id, status)) = self.current_attempt_for(&build) {
            return self.plan_existing(id, status);
        }
        if self.deployment_in_progress() {
            return Err(DeployError::DeploymentBusy);
        }
        self.stage(build)
    }

    /// Returns the action needed to converge the persisted deployment without
    /// introducing a new deployment intent.
    pub fn current_plan(&self) -> Result<DeployPlan, DeployError> {
        self.validate()?;
        Ok(self.next_plan())
    }

    fn stage(&mut self, build: BuildId) -> Result<DeployPlan, DeployError> {
        let id = GenerationId::from_raw(self.next_generation);
        let next_generation = self
            .next_generation
            .checked_add(1)
            .ok_or(DeployError::GenerationIdsExhausted)?;
        self.activation = None;
        self.next_generation = next_generation;
        self.generations.insert(
            id,
            Generation {
                build: build.clone(),
                status: GenerationStatus::Staged,
                active_connections: None,
                failure: None,
            },
        );
        Ok(DeployPlan::StageTarget { target: id, build })
    }

    /// Applies an observed host outcome and returns the next idempotent action.
    pub fn reconcile(&mut self, event: DeploymentEvent) -> Result<DeployPlan, DeployError> {
        self.validate()?;
        let before = self.clone();
        let outcome = (|| {
            match event {
                DeploymentEvent::Prepared { target } => self.prepared(target)?,
                DeploymentEvent::PreviousPaused { target } => self.previous_paused(target)?,
                DeploymentEvent::TargetAccepting { target } => self.target_accepting(target)?,
                DeploymentEvent::AdmissionsResumed { failed_target } => {
                    self.admissions_resumed(failed_target)?
                }
                DeploymentEvent::ConnectionsObserved { generation, active } => {
                    self.observe_connections(generation, active)?;
                }
                DeploymentEvent::Retired { generation } => self.retired(generation)?,
                DeploymentEvent::Failed { generation, reason } => {
                    self.failed(generation, reason)?
                }
            }
            self.validate()?;
            Ok(self.next_plan())
        })();
        if outcome.is_err() {
            *self = before;
        }
        outcome
    }

    pub fn snapshot(&self) -> DeploymentSnapshot {
        DeploymentSnapshot {
            generations: self
                .generations
                .iter()
                .map(|(&id, generation)| GenerationSnapshot {
                    id,
                    build: generation.build.clone(),
                    status: generation.status,
                    active_connections: generation.active_connections,
                    failure: generation.failure.clone(),
                })
                .collect(),
            activation: self
                .activation
                .as_ref()
                .map(|activation| ActivationSnapshot {
                    target: activation.target,
                    previous: activation.previous,
                    phase: activation.phase,
                    failure: activation.failure.clone(),
                }),
            last_rollback: self.last_rollback,
        }
    }
}
