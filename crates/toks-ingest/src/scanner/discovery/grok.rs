use std::path::{Path, PathBuf};

use crate::clients::ClientId;

use super::ScanPlan;

pub(super) fn add_primary_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::Grok) {
        return;
    }
    let sessions = PathBuf::from(
        ClientId::Grok
            .data()
            .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots),
    );
    add_root_tasks(plan, &sessions);
}

/// Register legacy rollups and inference logs for one Grok root.
pub(super) fn add_root_tasks(plan: &mut ScanPlan<'_>, root: &Path) {
    plan.push(ClientId::Grok, root);
    let grok_home = home_from_scan_root(root);
    plan.push_with_pattern(
        ClientId::Grok,
        grok_home.join("logs").join("unified.jsonl"),
        "unified.jsonl",
    );
}

fn home_from_scan_root(path: &Path) -> PathBuf {
    if let Some(sessions_dir) = path.ancestors().find(|candidate| {
        candidate
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case("sessions"))
    }) {
        if let Some(home) = sessions_dir.parent() {
            return home.to_path_buf();
        }
    }
    path.to_path_buf()
}
