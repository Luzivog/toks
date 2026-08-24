use crate::clients::ClientId;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if !plan.has(ClientId::Kimi) {
        return;
    }

    let legacy = ClientId::Kimi
        .data()
        .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
    plan.push(ClientId::Kimi, legacy);

    let kimi_code_home = if plan.use_env_roots {
        let configured = std::env::var("KIMI_CODE_HOME").unwrap_or_default();
        if configured.trim().is_empty() {
            join_native(plan.home_dir, ".kimi-code")
        } else {
            configured
        }
    } else {
        join_native(plan.home_dir, ".kimi-code")
    };
    plan.push(ClientId::Kimi, join_native(&kimi_code_home, "sessions"));
}
