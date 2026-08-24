use std::path::PathBuf;

use crate::clients::ClientId;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::Codebuff) && !plan.has(ClientId::Freebuff) {
        return;
    }
    let codebuff_override = env_override("CODEBUFF_DATA_DIR", plan.use_env_roots);
    let freebuff_override =
        env_override("FREEBUFF_DATA_DIR", plan.use_env_roots).or_else(|| codebuff_override.clone());

    let mut roots = Vec::new();
    if plan.has(ClientId::Codebuff) {
        roots.extend(manicode_roots(plan.home_dir, codebuff_override.as_deref()));
    }
    if plan.has(ClientId::Freebuff) {
        roots.extend(manicode_roots(plan.home_dir, freebuff_override.as_deref()));
    }
    for root in roots {
        plan.push(ClientId::Codebuff, root);
    }
}

fn env_override(name: &str, use_env_roots: bool) -> Option<String> {
    if !use_env_roots {
        return None;
    }
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn manicode_roots(home_dir: &str, override_root: Option<&str>) -> Vec<PathBuf> {
    match override_root {
        Some(root) => vec![PathBuf::from(join_native(root, "projects"))],
        None => {
            let config_dir = join_native(home_dir, ".config");
            ["manicode", "manicode-dev", "manicode-staging"]
                .iter()
                .map(|channel| {
                    PathBuf::from(join_native(&config_dir, &format!("{channel}/projects")))
                })
                .collect()
        }
    }
}
