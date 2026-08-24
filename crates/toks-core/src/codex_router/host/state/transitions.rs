use super::{
    Activation, ActivationPhase, DeployError, DeploymentState, GenerationId, GenerationStatus,
};

impl DeploymentState {
    pub(super) fn prepared(&mut self, target: GenerationId) -> Result<(), DeployError> {
        if self
            .activation
            .as_ref()
            .is_some_and(|active| active.target == target)
        {
            return Ok(());
        }
        if self.activation.is_some() {
            return Err(DeployError::DeploymentBusy);
        }
        if self.generation(target)?.status != GenerationStatus::Staged {
            return Err(DeployError::InvalidTransition("target is not staged"));
        }
        let previous = self.active_generation();
        self.activation = Some(Activation {
            target,
            previous,
            phase: ActivationPhase::Prepared,
            failure: None,
        });
        Ok(())
    }

    pub(super) fn previous_paused(&mut self, target: GenerationId) -> Result<(), DeployError> {
        let activation = self.activation_for(target)?;
        if activation.phase != ActivationPhase::Prepared {
            return Ok(());
        }
        if let Some(previous) = activation.previous {
            let generation = self.generation_mut(previous)?;
            match generation.status {
                GenerationStatus::Active => {
                    generation.status = GenerationStatus::Draining;
                    generation.active_connections = None;
                }
                GenerationStatus::Failed => {}
                GenerationStatus::Staged
                | GenerationStatus::Draining
                | GenerationStatus::Retired => {
                    return Err(DeployError::InvalidTransition(
                        "previous generation cannot be paused",
                    ));
                }
            }
        }
        self.activation.as_mut().expect("checked above").phase = ActivationPhase::PreviousPaused;
        Ok(())
    }

    pub(super) fn target_accepting(&mut self, target: GenerationId) -> Result<(), DeployError> {
        let activation = self.activation_for(target)?;
        if activation.phase == ActivationPhase::TargetAccepting {
            return Ok(());
        }
        if activation.phase != ActivationPhase::PreviousPaused || activation.failure.is_some() {
            return Err(DeployError::InvalidTransition("target cannot accept yet"));
        }
        self.generation_mut(target)?.status = GenerationStatus::Active;
        self.activation.as_mut().expect("checked above").phase = ActivationPhase::TargetAccepting;
        Ok(())
    }

    pub(super) fn admissions_resumed(
        &mut self,
        failed_target: GenerationId,
    ) -> Result<(), DeployError> {
        let Some(activation) = self.activation.as_ref() else {
            return self.idempotent_resumed(failed_target);
        };
        if activation.target != failed_target || activation.failure.is_none() {
            return Err(DeployError::InvalidTransition("rollback is not pending"));
        }
        let previous = activation
            .previous
            .ok_or(DeployError::InvalidTransition("no previous generation"))?;
        let previous_generation = self.generation_mut(previous)?;
        if previous_generation.status != GenerationStatus::Draining {
            return Err(DeployError::InvalidTransition(
                "previous generation cannot resume",
            ));
        }
        previous_generation.status = GenerationStatus::Active;
        self.activation = None;
        self.last_rollback = Some(failed_target);
        Ok(())
    }

    pub(super) fn observe_connections(
        &mut self,
        id: GenerationId,
        active: u64,
    ) -> Result<(), DeployError> {
        let generation = self.generation_mut(id)?;
        match generation.status {
            GenerationStatus::Active | GenerationStatus::Draining => {
                generation.active_connections = Some(active);
                Ok(())
            }
            GenerationStatus::Retired if active == 0 => Ok(()),
            GenerationStatus::Staged | GenerationStatus::Retired | GenerationStatus::Failed => Err(
                DeployError::InvalidTransition("generation cannot report active connections"),
            ),
        }
    }

    pub(super) fn retired(&mut self, id: GenerationId) -> Result<(), DeployError> {
        let generation = self.generation_mut(id)?;
        if generation.status == GenerationStatus::Retired {
            return Ok(());
        }
        if generation.status != GenerationStatus::Draining
            || generation.active_connections != Some(0)
        {
            return Err(DeployError::InvalidTransition("generation has not drained"));
        }
        generation.status = GenerationStatus::Retired;
        Ok(())
    }

    pub(super) fn failed(&mut self, id: GenerationId, reason: String) -> Result<(), DeployError> {
        match self.generation(id)?.status {
            GenerationStatus::Failed | GenerationStatus::Retired => return Ok(()),
            GenerationStatus::Staged | GenerationStatus::Active | GenerationStatus::Draining => {}
        }
        if reason.trim().is_empty() {
            return Err(DeployError::InvalidTransition("failure reason is empty"));
        }
        let is_target = self
            .activation
            .as_ref()
            .is_some_and(|active| active.target == id);
        let phase = self.activation.as_ref().map(|active| active.phase);
        let generation = self.generation_mut(id)?;
        generation.status = GenerationStatus::Failed;
        generation.failure = Some(reason.clone());
        if is_target {
            if phase == Some(ActivationPhase::Prepared) {
                self.activation = None;
            } else if let Some(activation) = self.activation.as_mut() {
                activation.failure = Some(reason);
            }
        }
        Ok(())
    }
}
