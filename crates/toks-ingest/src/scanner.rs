//! Parallel discovery of local client sessions and databases.

mod directory;
mod discovery;
mod settings;
mod types;

pub use directory::scan_directory;
pub use discovery::{
    built_in_extra_scan_paths_for, copilot_exporter_path, copilot_exporter_path_with_env_strategy,
    devin_desktop_additional_roots, headless_roots, headless_roots_with_env_strategy,
    prime_agent_session_roots_with_env_strategy, scan_all_clients,
    scan_all_clients_with_env_strategy, scan_all_clients_with_scanner_settings,
};
pub use settings::{extra_scan_paths_for, parse_extra_dirs, ScannerSettings};
pub use types::{CrushDbSource, ScanResult};

#[cfg(test)]
use std::collections::HashSet;
#[cfg(test)]
use std::path::{Path, PathBuf};

#[cfg(test)]
use crate::clients::ClientId;
#[cfg(test)]
use discovery::{
    discover_crush_dbs, discover_micode_dbs_in_dirs, discover_opencode_dbs,
    expand_tilde_path_with_home, is_micode_db_filename, is_opencode_db_filename, join_native,
    merge_user_opencode_db_paths, prime_agent_session_dir_from_settings_files, scan_crush_registry,
    PrimeSessionDirSetting,
};

#[cfg(test)]
mod rename_tests;

#[cfg(test)]
mod tests;
