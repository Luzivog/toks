use std::collections::HashSet;
use std::path::PathBuf;

use crate::clients::ClientId;
use crate::scanner::ScanResult;

use super::common::join_native;
use super::ScanPlan;

pub fn copilot_exporter_path_with_env_strategy(use_env_roots: bool) -> Option<PathBuf> {
    if !use_env_roots {
        return None;
    }
    let path = std::env::var("COPILOT_OTEL_FILE_EXPORTER_PATH").ok()?;
    let trimmed = path.trim();
    (!trimmed.is_empty()).then(|| PathBuf::from(trimmed))
}

pub fn copilot_exporter_path() -> Option<PathBuf> {
    copilot_exporter_path_with_env_strategy(true)
}

pub(super) fn finish_discovery(
    plan: &ScanPlan<'_>,
    result: &mut ScanResult,
    seen: &mut HashSet<PathBuf>,
) {
    if !plan.has(ClientId::Copilot) {
        return;
    }
    let desktop_db = PathBuf::from(join_native(plan.home_dir, ".copilot/data.db"));
    if desktop_db.is_file() {
        result.copilot_desktop_db = Some(desktop_db);
    }
    result.copilot_vscode_sessions = discover_vscode_sessions(plan.home_dir, plan.use_env_roots);

    if let Some(path) = copilot_exporter_path_with_env_strategy(plan.use_env_roots) {
        if path.is_file() && seen.insert(path.clone()) {
            let files = result.get_mut(ClientId::Copilot);
            files.push(path);
            files.sort_unstable();
        }
    }
}

fn discover_vscode_sessions(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut roots = vec![
        PathBuf::from(format!(
            "{home_dir}/Library/Application Support/Code/User/workspaceStorage"
        )),
        PathBuf::from(format!("{home_dir}/.config/Code/User/workspaceStorage")),
    ];
    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(app_data) = std::env::var_os("APPDATA").filter(|value| !value.is_empty()) {
            roots.push(PathBuf::from(app_data).join("Code/User/workspaceStorage"));
        }
    }
    roots.push(PathBuf::from(home_dir).join("AppData/Roaming/Code/User/workspaceStorage"));

    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for workspace_storage in roots {
        let hash_dirs = match std::fs::read_dir(workspace_storage) {
            Ok(entries) => entries,
            Err(_) => continue,
        };
        for entry in hash_dirs.filter_map(Result::ok) {
            let chat_sessions = entry.path().join("chatSessions");
            if !chat_sessions.is_dir() {
                continue;
            }
            let chat_entries = match std::fs::read_dir(chat_sessions) {
                Ok(entries) => entries,
                Err(_) => continue,
            };
            for chat_entry in chat_entries.filter_map(Result::ok) {
                let path = chat_entry.path();
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("");
                if !name.ends_with(".jsonl") || !path.is_file() {
                    continue;
                }
                let key = std::fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
                if seen.insert(key) {
                    files.push(path);
                }
            }
        }
    }
    files.sort_unstable();
    files
}
