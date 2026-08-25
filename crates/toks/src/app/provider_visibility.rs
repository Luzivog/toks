use toks_core::ClientId;

use super::ToksApp;

impl ToksApp {
    pub(crate) fn set_provider_visible(&mut self, provider: ClientId, visible: bool) {
        let mut updated = self.provider_visibility.clone();
        if !updated.set_visible(provider, visible) {
            return;
        }
        match toks_core::save_provider_visibility(&updated) {
            Ok(()) => {
                self.provider_visibility = updated;
                self.provider_visibility_error = None;
            }
            Err(error) => {
                self.provider_visibility_error =
                    Some(format!("Couldn't save provider settings: {error}"));
            }
        }
    }
}
