use crate::clients::ClientId;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::OpenClaw) {
        return;
    }

    let primary = ClientId::OpenClaw
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push(ClientId::OpenClaw, primary);
    for legacy in [".clawdbot/agents", ".moltbot/agents", ".moldbot/agents"] {
        plan.push(ClientId::OpenClaw, join_native(plan.home_dir, legacy));
    }
}
