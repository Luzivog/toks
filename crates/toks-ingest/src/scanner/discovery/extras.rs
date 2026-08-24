use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::clients::ClientId;

use super::common::{join_native, warn_if_escapes_home};
use super::grok::add_root_tasks;
use super::ScanPlan;
use crate::scanner::{extra_scan_paths_for, parse_extra_dirs, ScannerSettings};

pub fn built_in_extra_scan_paths_for(
    home_dir: &str,
    enabled: &HashSet<ClientId>,
) -> Vec<(ClientId, PathBuf)> {
    let mut paths = Vec::new();
    if enabled.contains(&ClientId::Claude) {
        paths.push((
            ClientId::Claude,
            PathBuf::from(join_native(home_dir, ".claude/transcripts")),
        ));
        paths.extend(
            crate::cc_mirror::discover_claude_project_roots(Path::new(home_dir))
                .into_iter()
                .map(|path| (ClientId::Claude, path)),
        );
    }
    paths
}

pub(super) fn add_settings(plan: &mut ScanPlan<'_>, settings: &ScannerSettings) {
    let paths = extra_scan_paths_for(settings, &plan.enabled_with_devin_lookup);
    for (client_id, path) in paths {
        warn_if_escapes_home(Path::new(plan.home_dir), client_id, &path);
        add_extra_path(plan, client_id, path);
    }
}

pub(super) fn add_builtin(plan: &mut ScanPlan<'_>) {
    for (client_id, path) in built_in_extra_scan_paths_for(plan.home_dir, &plan.enabled) {
        plan.push(client_id, path);
    }
}

pub(super) fn add_environment(plan: &mut ScanPlan<'_>) {
    if !plan.use_env_roots {
        return;
    }
    let value =
        crate::paths::renamed_env_var("TOKS_EXTRA_DIRS", "TOKSCOPE_EXTRA_DIRS").unwrap_or_default();
    let paths = parse_extra_dirs(&value, &plan.enabled_with_devin_lookup);
    for (client_id, path) in paths {
        let path = PathBuf::from(path);
        warn_if_escapes_home(Path::new(plan.home_dir), client_id, &path);
        add_extra_path(plan, client_id, path);
    }
}

fn add_extra_path(plan: &mut ScanPlan<'_>, client_id: ClientId, path: PathBuf) {
    match client_id {
        ClientId::DevinCli => plan.devin_cli_roots.push(path),
        ClientId::Grok => add_root_tasks(plan, &path),
        _ => plan.push(client_id, path),
    }
}
