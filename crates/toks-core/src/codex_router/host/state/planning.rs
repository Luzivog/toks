use super::{
    Activation, ActivationPhase, BuildId, DeployError, DeployPlan, DeploymentState, Generation,
    GenerationId, GenerationStatus,
};

impl DeploymentState {
    pub(super) fn current_attempt_for(
        &self,
        build: &BuildId,
    ) -> Option<(GenerationId, GenerationStatus)> {
        self.generations
            .iter()
            .find(|(_, generation)| {
                generation.build == *build && generation.status == GenerationStatus::Active
            })
            .or_else(|| {
                self.generations
                    .iter()
                    .rev()
                    .find(|(_, generation)| generation.build == *build)
                    .filter(|(_, generation)| {
                        // A newer non-reusable attempt shadows older history.
                        // Reusing an old id creates an ABA hazard: delayed reports
                        // from its earlier lifetime would target the revival.
                        !matches!(
                            generation.status,
                            GenerationStatus::Draining | GenerationStatus::Retired
                        )
                    })
            })
            .map(|(&id, generation)| (id, generation.status))
    }

    pub(super) fn plan_existing(
        &self,
        id: GenerationId,
        status: GenerationStatus,
    ) -> Result<DeployPlan, DeployError> {
        if self
            .activation
            .as_ref()
            .is_some_and(|activation| activation.target != id && self.deployment_in_progress())
        {
            return Err(DeployError::DeploymentBusy);
        }
        Ok(match status {
            GenerationStatus::Staged => self.next_plan(),
            GenerationStatus::Failed
                if self
                    .activation
                    .as_ref()
                    .is_some_and(|activation| activation.target == id) =>
            {
                self.next_plan()
            }
            GenerationStatus::Failed => DeployPlan::Unavailable {
                failed_target: id,
                active: self.active_generation(),
            },
            GenerationStatus::Active | GenerationStatus::Draining | GenerationStatus::Retired => {
                self.next_plan()
            }
        })
    }

    pub(super) fn next_plan(&self) -> DeployPlan {
        if let Some(activation) = &self.activation {
            if activation.failure.is_some() {
                return activation
                    .previous
                    .filter(|previous| {
                        self.generation(*previous)
                            .is_ok_and(|generation| generation.status == GenerationStatus::Draining)
                    })
                    .map_or(
                        DeployPlan::Unavailable {
                            failed_target: activation.target,
                            active: self.active_generation(),
                        },
                        |previous| DeployPlan::ResumeAdmissions {
                            previous,
                            failed_target: activation.target,
                        },
                    );
            }
            match activation.phase {
                ActivationPhase::Prepared => {
                    return DeployPlan::PauseAdmissions {
                        previous: activation.previous,
                        target: activation.target,
                    };
                }
                ActivationPhase::PreviousPaused => {
                    return DeployPlan::StartAccepting {
                        target: activation.target,
                    };
                }
                ActivationPhase::TargetAccepting => {}
            }
        }
        if let Some((&id, generation)) = self
            .generations
            .iter()
            .find(|(_, generation)| generation.status == GenerationStatus::Staged)
        {
            return DeployPlan::StageTarget {
                target: id,
                build: generation.build.clone(),
            };
        }
        if let Some((&generation, _)) = self.generations.iter().find(|(_, generation)| {
            generation.status == GenerationStatus::Draining
                && generation.active_connections == Some(0)
        }) {
            return DeployPlan::Retire { generation };
        }
        let active = self.active_generation();
        if active.is_none() {
            if let Some((&failed_target, _)) = self
                .generations
                .iter()
                .rev()
                .find(|(_, generation)| generation.status == GenerationStatus::Failed)
            {
                return DeployPlan::Unavailable {
                    failed_target,
                    active: None,
                };
            }
        }
        DeployPlan::Settled { active }
    }

    pub(super) fn deployment_in_progress(&self) -> bool {
        self.activation.as_ref().is_some_and(|activation| {
            if activation.failure.is_some() {
                activation.previous.is_some_and(|previous| {
                    self.generation(previous)
                        .is_ok_and(|generation| generation.status == GenerationStatus::Draining)
                })
            } else {
                activation.phase != ActivationPhase::TargetAccepting
            }
        }) || self
            .generations
            .values()
            .any(|generation| generation.status == GenerationStatus::Staged)
    }

    pub(super) fn active_generation(&self) -> Option<GenerationId> {
        self.generations.iter().find_map(|(&id, generation)| {
            (generation.status == GenerationStatus::Active).then_some(id)
        })
    }

    pub(super) fn generation(&self, id: GenerationId) -> Result<&Generation, DeployError> {
        self.generations
            .get(&id)
            .ok_or(DeployError::UnknownGeneration(id))
    }

    pub(super) fn generation_mut(
        &mut self,
        id: GenerationId,
    ) -> Result<&mut Generation, DeployError> {
        self.generations
            .get_mut(&id)
            .ok_or(DeployError::UnknownGeneration(id))
    }

    pub(super) fn activation_for(&self, target: GenerationId) -> Result<&Activation, DeployError> {
        self.activation
            .as_ref()
            .filter(|activation| activation.target == target)
            .ok_or(DeployError::InvalidTransition("activation target mismatch"))
    }

    pub(super) fn idempotent_resumed(
        &self,
        failed_target: GenerationId,
    ) -> Result<(), DeployError> {
        if self.last_rollback == Some(failed_target) {
            Ok(())
        } else {
            Err(DeployError::InvalidTransition("rollback is not pending"))
        }
    }
}
