use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::clients::ClientId;
use crate::scanner::{ScanResult, ScannerSettings};

use super::common::join_native;
use super::ScanPlan;

/// Discover the default and channel-specific OpenCode databases.
pub(crate) fn discover_opencode_dbs(data_dir: &Path) -> Vec<PathBuf> {
    let entries = match std::fs::read_dir(data_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };
    let mut databases: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let file_type = entry.file_type().ok()?;
            if !file_type.is_file() && !entry.path().is_file() {
                return None;
            }
            let path = entry.path();
            let name = path.file_name()?.to_str()?;
            is_opencode_db_filename(name).then_some(path)
        })
        .collect();
    databases.sort_unstable();
    databases
}

pub(in crate::scanner) fn is_opencode_db_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".db") else {
        return false;
    };
    if stem == "opencode" {
        return true;
    }
    let Some(channel) = stem.strip_prefix("opencode-") else {
        return false;
    };
    !channel.is_empty()
        && channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// Merge valid user-configured database paths into auto-discovery.
pub(crate) fn merge_user_opencode_db_paths(discovered: &mut Vec<PathBuf>, extra_paths: &[PathBuf]) {
    if extra_paths.is_empty() {
        return;
    }
    let mut seen: HashSet<PathBuf> = discovered
        .iter()
        .map(|path| std::fs::canonicalize(path).unwrap_or_else(|_| path.clone()))
        .collect();
    for path in extra_paths {
        if !path.is_file() {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !is_opencode_db_filename(name) {
            continue;
        }
        let canonical = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        if seen.insert(canonical) {
            discovered.push(path.clone());
        }
    }
}

pub(super) fn discover(
    plan: &mut ScanPlan<'_>,
    result: &mut ScanResult,
    settings: &ScannerSettings,
) {
    if !plan.has(ClientId::OpenCode) {
        return;
    }
    let xdg_data = if plan.use_env_roots {
        std::env::var("XDG_DATA_HOME")
            .unwrap_or_else(|_| join_native(plan.home_dir, ".local/share"))
    } else {
        join_native(plan.home_dir, ".local/share")
    };
    let data_dir = PathBuf::from(join_native(&xdg_data, "opencode"));
    result.opencode_dbs = discover_opencode_dbs(&data_dir);
    merge_user_opencode_db_paths(&mut result.opencode_dbs, &settings.opencode_db_paths);
    result.opencode_dbs.sort_unstable();
    result.opencode_dbs.dedup();

    let legacy = ClientId::OpenCode
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    result.opencode_json_dir = Some(PathBuf::from(&legacy));
    plan.push(ClientId::OpenCode, legacy);
}
