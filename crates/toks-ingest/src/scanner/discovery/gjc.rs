use std::path::PathBuf;

use crate::clients::ClientId;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::Gjc) {
        return;
    }
    let mut roots = vec![PathBuf::from(
        ClientId::Gjc
            .data()
            .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots),
    )];

    if plan.use_env_roots {
        for name in ["GJC_CONFIG_DIR", "PI_CONFIG_DIR"] {
            if let Ok(config_dir) = std::env::var(name) {
                let trimmed = config_dir.trim();
                if !trimmed.is_empty() {
                    roots.push(PathBuf::from(trimmed.trim_end_matches('/')).join("agent/sessions"));
                }
            }
        }
        #[cfg(any(target_os = "linux", target_os = "macos"))]
        if let Ok(xdg_data) = std::env::var("XDG_DATA_HOME") {
            let trimmed = xdg_data.trim();
            if !trimmed.is_empty() {
                roots.push(PathBuf::from(trimmed.trim_end_matches('/')).join("gjc/sessions"));
            }
        }
    }
    roots.push(PathBuf::from(join_native(
        plan.home_dir,
        ".gjc/agent/sessions",
    )));

    for root in roots {
        if root.exists() {
            plan.push(ClientId::Gjc, root);
        }
    }
}
