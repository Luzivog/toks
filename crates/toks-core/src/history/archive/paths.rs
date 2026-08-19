use std::path::PathBuf;

pub(super) fn default_path() -> Option<PathBuf> {
    toks_ingest::paths::get_data_dir().map(|root| root.join("history/usage.sqlite3"))
}
