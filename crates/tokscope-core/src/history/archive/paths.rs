use std::path::PathBuf;

pub(super) fn default_path() -> Option<PathBuf> {
    dirs::data_local_dir()
        .or_else(dirs::data_dir)
        .map(|root| root.join("tokscope/history/usage.sqlite3"))
}
