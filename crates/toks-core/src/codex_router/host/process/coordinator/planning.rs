use anyhow::Result;

use crate::codex_router::host::DeployPlan;

use super::core::Coordinator;

impl Coordinator {
    /// Reconciles an older persisted activation before introducing the build
    /// represented by this coordinator process.
    pub(super) fn plan_for_advance(&mut self) -> Result<DeployPlan> {
        let current = self.deployment.current_plan()?;
        if !is_terminal(&current) {
            return Ok(current);
        }
        if self.consume_retry_intent()? {
            return Ok(self.deployment.current_plan()?);
        }
        Ok(self.deployment.plan_deploy(self.build.clone())?)
    }

    pub(super) fn current_plan(&self) -> Result<DeployPlan> {
        Ok(self.deployment.current_plan()?)
    }
}

fn is_terminal(plan: &DeployPlan) -> bool {
    matches!(
        plan,
        DeployPlan::Settled { .. } | DeployPlan::Unavailable { .. }
    )
}
