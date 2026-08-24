use std::path::PathBuf;

use crate::clients::ClientId;
use crate::scanner::ScanResult;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn discover(plan: &mut ScanPlan<'_>, result: &mut ScanResult) {
    if !plan.has(ClientId::Kiro) {
        return;
    }
    let cli = ClientId::Kiro
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push_with_pattern(ClientId::Kiro, cli, "*.json");

    for root in global_storage_roots(plan.home_dir, plan.use_env_roots) {
        plan.push_with_pattern(ClientId::Kiro, root, "kiro-globalstorage");
    }
    plan.push_with_pattern(
        ClientId::Kiro,
        PathBuf::from(join_native(plan.home_dir, ".kiro/sessions")),
        "kiro-ide-session",
    );

    let xdg = PathBuf::from(join_native(
        plan.home_dir,
        ".local/share/kiro-cli/data.sqlite3",
    ));
    if xdg.is_file() {
        result.kiro_db = Some(xdg);
    }
    if result.kiro_db.is_none() {
        let macos = PathBuf::from(format!(
            "{}/Library/Application Support/kiro-cli/data.sqlite3",
            plan.home_dir
        ));
        if macos.is_file() {
            result.kiro_db = Some(macos);
        }
    }
}

fn global_storage_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(format!(
            "{home_dir}/Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent"
        )),
        PathBuf::from(format!(
            "{home_dir}/Library/Application Support/kiro/User/globalStorage/kiro.kiroagent"
        )),
        PathBuf::from(format!(
            "{home_dir}/.config/Kiro/User/globalStorage/kiro.kiroagent"
        )),
        PathBuf::from(format!(
            "{home_dir}/.config/kiro/User/globalStorage/kiro.kiroagent"
        )),
    ];
    if cfg!(target_os = "windows") {
        if use_env_roots {
            if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
                roots.push(PathBuf::from(&app_data).join("Kiro/User/globalStorage/kiro.kiroagent"));
                roots.push(PathBuf::from(&app_data).join("kiro/User/globalStorage/kiro.kiroagent"));
            }
        }
        roots.push(PathBuf::from(format!(
            "{home_dir}/AppData/Roaming/Kiro/User/globalStorage/kiro.kiroagent"
        )));
        roots.push(PathBuf::from(format!(
            "{home_dir}/AppData/Roaming/kiro/User/globalStorage/kiro.kiroagent"
        )));
    }
    roots
}
