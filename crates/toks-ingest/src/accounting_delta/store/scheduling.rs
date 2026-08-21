use super::{metadata, CheckpointStore};

impl CheckpointStore {
    pub(in crate::accounting_delta) fn rotation_cursor(&self) -> Result<Option<String>, String> {
        metadata::load_rotation_cursor(&self.connection)
    }
}
