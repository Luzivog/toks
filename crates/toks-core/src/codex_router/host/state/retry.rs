use super::{BuildId, DeployError, DeployPlan, DeploymentState, GenerationStatus, RetryReceipt};
use crate::codex_router::host::RetryId;

impl DeploymentState {
    /// Explicitly retries a failed build with a fresh generation id.
    /// `None` keeps the request pending while an earlier rollback settles.
    pub(in crate::codex_router::host) fn retry_deploy(
        &mut self,
        build: BuildId,
    ) -> Result<Option<DeployPlan>, DeployError> {
        self.validate()?;
        if self.deployment_in_progress() {
            let tracked = self.generations.values().any(|generation| {
                generation.build == build
                    && matches!(
                        generation.status,
                        GenerationStatus::Staged | GenerationStatus::Active
                    )
            });
            return tracked.then(|| self.plan_deploy(build)).transpose();
        }
        if self.generations.values().any(|generation| {
            generation.build == build && generation.status == GenerationStatus::Active
        }) {
            return self.plan_deploy(build).map(Some);
        }
        let failed = self.generations.values().any(|generation| {
            generation.build == build && generation.status == GenerationStatus::Failed
        });
        if !failed {
            return self.plan_deploy(build).map(Some);
        }
        self.stage(build).map(Some)
    }

    /// Atomically binds one durable installer intent to the generation it
    /// created. Replaying an already-bound intent never allocates again.
    pub(crate) fn consume_retry(
        &mut self,
        build: BuildId,
        id: RetryId,
    ) -> Result<bool, DeployError> {
        self.validate()?;
        if let Some(receipt) = self.retry_receipts.get(&id) {
            return if receipt.build == build {
                Ok(true)
            } else {
                Err(DeployError::InvalidTransition("retry id build mismatch"))
            };
        }
        let Some(plan) = self.retry_deploy(build.clone())? else {
            return Ok(false);
        };
        let generation = match plan {
            DeployPlan::StageTarget { target, .. } => target,
            DeployPlan::Settled {
                active: Some(active),
            } => active,
            _ => {
                return Err(DeployError::InvalidTransition(
                    "retry did not bind to a generation",
                ));
            }
        };
        self.retry_receipts
            .insert(id, RetryReceipt { build, generation });
        Ok(true)
    }
}
