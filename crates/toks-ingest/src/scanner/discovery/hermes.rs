use std::path::{Path, PathBuf};

use crate::clients::ClientId;
use crate::scanner::ScanResult;

use super::ScanPlan;

/// Find named-profile databases when scanning a root Hermes home.
pub(crate) fn discover_hermes_profile_state_dbs(hermes_home: &Path) -> Vec<PathBuf> {
    if hermes_home
        .parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "profiles")
    {
        return Vec::new();
    }
    let mut databases: Vec<PathBuf> = std::fs::read_dir(hermes_home.join("profiles"))
        .into_iter()
        .flat_map(|entries| entries.filter_map(Result::ok))
        .filter_map(|entry| {
            let state_db = entry.path().join("state.db");
            state_db.is_file().then_some(state_db)
        })
        .collect();
    databases.sort_unstable();
    databases.dedup();
    databases
}

fn home_candidates(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut homes = vec![PathBuf::from(
        ClientId::Hermes
            .data()
            .root
            .resolve_with_env_strategy(home_dir, use_env_roots),
    )];
    let explicit_home = use_env_roots
        && std::env::var("HERMES_HOME")
            .map(|value| !value.trim().is_empty())
            .unwrap_or(false);
    if !explicit_home {
        if cfg!(target_os = "windows") && use_env_roots {
            if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty())
            {
                homes.push(PathBuf::from(local).join("hermes"));
            }
        }
        homes.push(PathBuf::from(home_dir).join("AppData/Local/hermes"));
    }
    homes
}

pub(super) fn discover(plan: &mut ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::Hermes) {
        return;
    }
    let mut extra_databases = Vec::new();
    for home in home_candidates(plan.home_dir, plan.use_env_roots) {
        let default_db = home.join("state.db");
        if default_db.is_file() {
            if result.hermes_db.is_none() {
                result.hermes_db = Some(default_db);
            } else if result.hermes_db.as_ref() != Some(&default_db) {
                extra_databases.push(default_db);
            }
        }
        extra_databases.extend(discover_hermes_profile_state_dbs(&home));
    }
    extra_databases.sort_unstable();
    extra_databases.dedup();
    result.get_mut(ClientId::Hermes).extend(extra_databases);
}
