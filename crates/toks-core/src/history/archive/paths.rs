use std::path::PathBuf;

pub(super) fn default_path() -> Option<PathBuf> {
    crate::paths::history_archive().ok()
}
