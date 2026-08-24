use std::path::PathBuf;

use crate::clients::ClientId;
use crate::scanner::{scan_directory, ScanResult};

use super::common::dedup_dbs_by_canonical_path;
use super::ScanPlan;

pub fn devin_desktop_additional_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(home_dir).join(".config/Devin/User/acp-events"),
        PathBuf::from(home_dir).join(".config/devin/User/acp-events"),
    ];
    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(app_data).join("Devin/User/acp-events"));
        }
    }
    roots.push(PathBuf::from(home_dir).join("AppData/Roaming/Devin/User/acp-events"));
    roots
}

pub(super) fn add_desktop_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::DevinDesktop) {
        return;
    }
    let primary = ClientId::DevinDesktop
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push(ClientId::DevinDesktop, primary);
    for root in devin_desktop_additional_roots(plan.home_dir, plan.use_env_roots) {
        plan.push(ClientId::DevinDesktop, root);
    }
}

pub(super) fn discover_databases(plan: &mut ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::DevinCli) && !plan.has(ClientId::DevinDesktop) {
        return;
    }
    let primary = ClientId::DevinCli
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.devin_cli_roots.push(PathBuf::from(primary));
    result.devin_dbs = dedup_dbs_by_canonical_path(
        std::mem::take(&mut plan.devin_cli_roots)
            .into_iter()
            .flat_map(|root| scan_directory(&root.to_string_lossy(), "sessions.db")),
    );
}
