use std::path::{Path, PathBuf};

use serde::Deserialize;
use serde_json::Value;

use crate::clients::ClientId;
use crate::scanner::{CrushDbSource, ScanResult};
use crate::sessions::{normalize_workspace_key, workspace_label_from_key};

use super::ScanPlan;

#[derive(Debug, Deserialize, Default)]
struct CrushProjectList {
    #[serde(default)]
    projects: Vec<Value>,
}

#[derive(Debug, Deserialize)]
struct CrushProject {
    path: String,
    data_dir: String,
}

fn crush_db_path(data_dir: &Path) -> Option<PathBuf> {
    let candidate = data_dir.join("crush.db");
    candidate.is_file().then_some(candidate)
}

fn resolve_data_dir(project: &CrushProject) -> PathBuf {
    let data_dir = PathBuf::from(&project.data_dir);
    if data_dir.is_absolute() {
        data_dir
    } else {
        PathBuf::from(&project.path).join(data_dir)
    }
}

pub(in crate::scanner) fn scan_crush_registry(registry_path: &Path) -> Vec<CrushDbSource> {
    let registry = match std::fs::read_to_string(registry_path) {
        Ok(contents) => contents,
        Err(_) => return Vec::new(),
    };
    let list: CrushProjectList = match serde_json::from_str(&registry) {
        Ok(list) => list,
        Err(_) => return Vec::new(),
    };
    list.projects
        .into_iter()
        .filter_map(|project| serde_json::from_value::<CrushProject>(project).ok())
        .filter_map(|project| {
            let db_path = crush_db_path(&resolve_data_dir(&project))?;
            let workspace_key = normalize_workspace_key(&project.path);
            let workspace_label = workspace_key.as_deref().and_then(workspace_label_from_key);
            Some(CrushDbSource {
                db_path,
                workspace_key,
                workspace_label,
            })
        })
        .collect()
}

fn registry_candidates(home_dir: &str, use_env_roots: bool) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if use_env_roots {
        if let Some(global_data) =
            std::env::var_os("CRUSH_GLOBAL_DATA").filter(|value| !value.is_empty())
        {
            candidates.push(PathBuf::from(global_data).join("projects.json"));
        }
    }
    candidates.push(PathBuf::from(
        ClientId::Crush
            .data()
            .resolve_path_with_env_strategy(home_dir, use_env_roots),
    ));
    if cfg!(target_os = "windows") && use_env_roots {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").filter(|value| !value.is_empty()) {
            candidates.push(PathBuf::from(local).join("crush").join("projects.json"));
        }
    }
    candidates.push(PathBuf::from(home_dir).join("AppData/Local/crush/projects.json"));
    candidates
}

pub(in crate::scanner) fn discover_crush_dbs(
    home_dir: &str,
    use_env_roots: bool,
) -> Vec<CrushDbSource> {
    let mut databases = Vec::new();
    for registry in registry_candidates(home_dir, use_env_roots) {
        databases.extend(scan_crush_registry(&registry));
    }
    databases.sort_by(|left, right| left.db_path.cmp(&right.db_path));
    databases.dedup_by(|left, right| left.db_path == right.db_path);
    databases
}

pub(super) fn discover(plan: &ScanPlan<'_>, result: &mut ScanResult) {
    if plan.has(ClientId::Crush) {
        result.crush_dbs = discover_crush_dbs(plan.home_dir, plan.use_env_roots);
    }
}
