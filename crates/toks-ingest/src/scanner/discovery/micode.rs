use std::path::{Path, PathBuf};

use crate::clients::ClientId;
use crate::scanner::ScanResult;

use super::common::dedup_dbs_by_canonical_path;
use super::ScanPlan;

pub(crate) fn discover_micode_dbs(data_dir: &Path) -> Vec<PathBuf> {
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
            is_micode_db_filename(name).then_some(path)
        })
        .collect();
    databases.sort_unstable();
    databases
}

pub(crate) fn discover_micode_dbs_in_dirs(dirs: impl IntoIterator<Item = PathBuf>) -> Vec<PathBuf> {
    dedup_dbs_by_canonical_path(
        dirs.into_iter()
            .flat_map(|directory| discover_micode_dbs(&directory)),
    )
}

pub(in crate::scanner) fn is_micode_db_filename(name: &str) -> bool {
    let Some(stem) = name.strip_suffix(".db") else {
        return false;
    };
    if stem == "mimocode" {
        return true;
    }
    let Some(channel) = stem.strip_prefix("mimocode-") else {
        return false;
    };
    !channel.is_empty()
        && channel
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

pub(super) fn discover(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::MiMoCode) {
        return;
    }
    let primary = PathBuf::from(
        ClientId::MiMoCode
            .data()
            .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots),
    );
    let orca = PathBuf::from(format!(
        "{}/Library/Application Support/orca/mimocode-hooks/shared/data",
        plan.home_dir
    ));
    result.micode_dbs = discover_micode_dbs_in_dirs([primary, orca]);
}
