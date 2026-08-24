use super::{ActivationPhase, DeployError, DeploymentState, GenerationStatus, STATE_VERSION};

impl DeploymentState {
    pub fn validate(&self) -> Result<(), DeployError> {
        if self.version != STATE_VERSION {
            return Err(DeployError::InvalidPersistedState("unsupported version"));
        }
        if self.next_generation == 0
            || self
                .generations
                .keys()
                .any(|id| id.get() == 0 || id.get() >= self.next_generation)
        {
            return Err(DeployError::InvalidPersistedState("invalid generation id"));
        }
        if self
            .generations
            .values()
            .any(|generation| generation.build.as_str().trim().is_empty())
        {
            return Err(DeployError::InvalidPersistedState("empty build id"));
        }
        // Generations are deployment attempts, so a later generation may redeploy
        // the same immutable build after its earlier attempt was retired.
        if self
            .generations
            .values()
            .filter(|generation| generation.status == GenerationStatus::Active)
            .count()
            > 1
        {
            return Err(DeployError::InvalidPersistedState(
                "multiple active generations",
            ));
        }
        if self
            .generations
            .values()
            .filter(|generation| generation.status == GenerationStatus::Staged)
            .count()
            > 1
        {
            return Err(DeployError::InvalidPersistedState(
                "multiple staged generations",
            ));
        }
        for generation in self.generations.values() {
            let failure_valid = match generation.status {
                GenerationStatus::Failed => generation
                    .failure
                    .as_ref()
                    .is_some_and(|reason| !reason.trim().is_empty()),
                _ => generation.failure.is_none(),
            };
            if !failure_valid {
                return Err(DeployError::InvalidPersistedState(
                    "generation failure contradicts status",
                ));
            }
        }
        if self
            .last_rollback
            .is_some_and(|id| match self.generation(id) {
                Ok(generation) => generation.status != GenerationStatus::Failed,
                Err(_) => true,
            })
        {
            return Err(DeployError::InvalidPersistedState(
                "invalid rollback receipt",
            ));
        }
        if self.retry_receipts.iter().any(|(id, receipt)| {
            !id.is_valid()
                || self
                    .generations
                    .get(&receipt.generation)
                    .is_none_or(|generation| generation.build != receipt.build)
        }) {
            return Err(DeployError::InvalidPersistedState("invalid retry receipt"));
        }
        self.validate_activation()
    }

    fn validate_activation(&self) -> Result<(), DeployError> {
        let Some(activation) = &self.activation else {
            return Ok(());
        };
        let target = self.generation(activation.target)?;
        if activation.previous == Some(activation.target) {
            return Err(DeployError::InvalidPersistedState("target equals previous"));
        }
        let previous_status = activation
            .previous
            .map(|id| self.generation(id).map(|generation| generation.status))
            .transpose()?;
        if activation.failure.is_some() {
            let previous_valid = previous_status.is_none_or(|status| {
                matches!(
                    status,
                    GenerationStatus::Draining
                        | GenerationStatus::Retired
                        | GenerationStatus::Failed
                )
            });
            if target.status != GenerationStatus::Failed
                || target.failure != activation.failure
                || activation.phase == ActivationPhase::Prepared
                || !previous_valid
            {
                return Err(DeployError::InvalidPersistedState(
                    "invalid failed activation",
                ));
            }
            return Ok(());
        }
        let valid = match activation.phase {
            ActivationPhase::Prepared => {
                target.status == GenerationStatus::Staged
                    && previous_status.is_none_or(|status| {
                        matches!(status, GenerationStatus::Active | GenerationStatus::Failed)
                    })
            }
            ActivationPhase::PreviousPaused => {
                target.status == GenerationStatus::Staged
                    && previous_status.is_none_or(|status| {
                        matches!(
                            status,
                            GenerationStatus::Draining | GenerationStatus::Failed
                        )
                    })
            }
            ActivationPhase::TargetAccepting => {
                target.status == GenerationStatus::Active
                    && previous_status.is_none_or(|status| {
                        matches!(
                            status,
                            GenerationStatus::Draining
                                | GenerationStatus::Retired
                                | GenerationStatus::Failed
                        )
                    })
            }
        };
        if valid {
            Ok(())
        } else {
            Err(DeployError::InvalidPersistedState(
                "activation contradicts generations",
            ))
        }
    }
}
