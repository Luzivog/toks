use anyhow::Result;

use crate::rotation::UnixMillis;
use crate::storage::StoreUpdate;

use super::Engine;

impl Engine {
    pub(super) fn reconcile_thread_overrides(&self, now: UnixMillis) -> Result<()> {
        self.settings.update(|settings| {
            match self
                .runtime
                .latest(|runtime| settings.reconcile_thread_overrides(runtime, now))
            {
                Ok(changed) => StoreUpdate::from_changed(Ok(()), changed),
                Err(error) => StoreUpdate::Unchanged(Err(error)),
            }
        })?
    }
}
