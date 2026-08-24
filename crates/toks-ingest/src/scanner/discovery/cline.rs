use std::path::PathBuf;

use crate::clients::ClientId;

use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    add_vscode_family(plan, ClientId::RooCode, "rooveterinaryinc.roo-cline/tasks");
    add_vscode_family(plan, ClientId::KiloCode, "kilocode.kilo-code/tasks");
    add_cline(plan);
}

fn add_vscode_family(plan: &mut ScanPlan<'_>, client_id: ClientId, server_relative: &str) {
    if !plan.has(client_id) {
        return;
    }
    let local = client_id
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push(client_id, local);
    plan.push(
        client_id,
        format!(
            "{}/.vscode-server/data/User/globalStorage/{server_relative}",
            plan.home_dir
        ),
    );
}

fn add_cline(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::Cline) {
        return;
    }
    let local = ClientId::Cline
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push(ClientId::Cline, local);
    for root in additional_vscode_task_roots(plan.home_dir, plan.use_env_roots) {
        plan.push(ClientId::Cline, root);
    }
    for root in cli_session_roots(plan.home_dir, plan.use_env_roots) {
        plan.push_with_pattern(ClientId::Cline, root, "cline-cli-messages");
    }
}

fn additional_vscode_task_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![PathBuf::from(home_dir)
        .join("Library/Application Support/Code/User/globalStorage/saoudrizwan.claude-dev/tasks")];
    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            roots.push(
                PathBuf::from(app_data)
                    .join("Code/User/globalStorage/saoudrizwan.claude-dev/tasks"),
            );
        }
    }
    roots.push(
        PathBuf::from(home_dir)
            .join("AppData/Roaming/Code/User/globalStorage/saoudrizwan.claude-dev/tasks"),
    );
    roots.push(
        PathBuf::from(home_dir)
            .join(".vscode-server/data/User/globalStorage/saoudrizwan.claude-dev/tasks"),
    );
    roots
}

fn cli_session_roots(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let fallback = || PathBuf::from(home_dir).join(".cline/data/sessions");
    if !use_env_roots {
        return vec![fallback()];
    }
    let env_path = |name: &str| {
        std::env::var_os(name)
            .filter(|value| !value.to_string_lossy().trim().is_empty())
            .map(PathBuf::from)
    };
    if let Some(path) = env_path("CLINE_SESSION_DATA_DIR") {
        return vec![path];
    }
    if let Some(path) = env_path("CLINE_DATA_DIR") {
        return vec![path.join("sessions")];
    }
    if let Some(path) = env_path("CLINE_DIR") {
        return vec![path.join("data/sessions")];
    }
    vec![fallback()]
}
