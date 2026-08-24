use anyhow::Result;

use super::core::Coordinator;
use crate::codex_router::host::process::paths::save_state;

impl Coordinator {
    pub(super) fn consume_retry_intent(&mut self) -> Result<bool> {
        let Some(intent) = crate::codex_router::host::load_retry_intent(&self.paths.state)? else {
            return Ok(false);
        };
        if intent.build != self.build
            || !self
                .deployment
                .consume_retry(self.build.clone(), intent.id.clone())?
        {
            return Ok(false);
        }
        save_state(&self.paths.state, &self.deployment)?;
        crate::codex_router::host::clear_retry_intent(&self.paths.state, &intent)?;
        self.consumed_retry_intent = Some(intent);
        Ok(true)
    }
}
