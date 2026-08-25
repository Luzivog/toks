use anyhow::Result;

use crate::rotation::UnixMillis;
use crate::storage::StoreUpdate;

use super::Engine;

impl Engine {
    pub(in crate::codex_router::proxy) fn apply_rotation_settings(
        &self,
        now: UnixMillis,
    ) -> Result<()> {
        self.settings.update(|settings| {
            match self.runtime.update(|runtime| {
                let dismissed = runtime.dismiss_cancelled_threads(settings.cancelled_threads());
                let settings_changed = settings.reconcile_thread_state(runtime)
                    | settings.reconcile_thread_overrides(runtime, now);
                StoreUpdate::from_changed(settings_changed, !dismissed.is_empty())
            }) {
                Ok(changed) => StoreUpdate::from_changed(Ok(()), changed),
                Err(error) => StoreUpdate::Unchanged(Err(error)),
            }
        })?
    }
}
