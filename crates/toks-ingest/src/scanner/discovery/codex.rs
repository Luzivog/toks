use crate::clients::ClientId;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::Codex) {
        return;
    }

    let codex_home = if plan.use_env_roots {
        std::env::var("CODEX_HOME").unwrap_or_else(|_| join_native(plan.home_dir, ".codex"))
    } else {
        join_native(plan.home_dir, ".codex")
    };
    let sessions = ClientId::Codex
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push(ClientId::Codex, sessions);
    plan.push(
        ClientId::Codex,
        join_native(&codex_home, "archived_sessions"),
    );

    for root in plan.headless_roots.clone() {
        plan.push(ClientId::Codex, root.join("codex"));
    }
}
