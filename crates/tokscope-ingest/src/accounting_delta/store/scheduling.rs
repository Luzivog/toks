use super::{save_state, CheckpointStore};
use crate::accounting_delta::SourceKey;

impl CheckpointStore {
    pub(in crate::accounting_delta) fn rotation_cursor(&self) -> Option<&str> {
        self.state.rotation_cursor.as_deref()
    }

    pub(in crate::accounting_delta) fn set_rotation_cursor(
        &mut self,
        source: &SourceKey,
    ) -> Result<(), String> {
        let mut next = self.state.clone();
        next.rotation_cursor = Some(source.as_str().to_string());
        save_state(&self.directory.join(super::STATE_FILE), &next)?;
        self.state = next;
        Ok(())
    }
}
