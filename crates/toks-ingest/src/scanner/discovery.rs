mod cline;
mod codebuddy;
mod codebuff;
mod codex;
mod common;
mod copilot;
mod crush;
mod databases;
mod devin;
mod extras;
mod generic;
mod gjc;
mod grok;
mod headless;
mod hermes;
mod kimi;
mod kiro;
mod micode;
mod openclaw;
mod opencode;
mod orchestration;
mod pi;
mod prime;

use common::ScanPlan;
pub use copilot::{copilot_exporter_path, copilot_exporter_path_with_env_strategy};
pub use devin::devin_desktop_additional_roots;
pub use extras::built_in_extra_scan_paths_for;
pub use headless::{headless_roots, headless_roots_with_env_strategy};
pub use orchestration::{
    scan_all_clients, scan_all_clients_with_env_strategy, scan_all_clients_with_scanner_settings,
};
pub use prime::prime_agent_session_roots_with_env_strategy;

use cline::add_tasks as add_cline_tasks;
use codebuddy::add_tasks as add_codebuddy_tasks;
use codebuff::add_tasks as add_codebuff_tasks;
use codex::add_tasks as add_codex_tasks;
use copilot::finish_discovery as finish_copilot_discovery;
use crush::discover as discover_crush;
use databases::{discover_goose, discover_kilo, discover_synthetic, discover_zcode, discover_zed};
use devin::{add_desktop_tasks, discover_databases as discover_devin_databases};
use extras::{add_builtin as add_builtin_extras, add_environment, add_settings};
use generic::{add_default_tasks, add_workbuddy_tasks};
use gjc::add_tasks as add_gjc_tasks;
use grok::add_primary_tasks as add_grok_tasks;
use hermes::discover as discover_hermes;
use kimi::add_tasks as add_kimi_tasks;
use kiro::discover as discover_kiro;
use micode::discover as discover_micode;
use openclaw::add_tasks as add_openclaw_tasks;
use opencode::discover as discover_opencode;
use pi::add_tasks as add_pi_tasks;
use prime::add_tasks as add_prime_tasks;

#[cfg(test)]
pub(super) use common::join_native;
#[cfg(test)]
pub(super) use crush::{discover_crush_dbs, scan_crush_registry};
#[cfg(test)]
pub(super) use micode::{discover_micode_dbs_in_dirs, is_micode_db_filename};
#[cfg(test)]
pub(super) use opencode::{
    discover_opencode_dbs, is_opencode_db_filename, merge_user_opencode_db_paths,
};
#[cfg(test)]
pub(super) use prime::{
    expand_tilde_path_with_home, prime_agent_session_dir_from_settings_files,
    PrimeSessionDirSetting,
};
