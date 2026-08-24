use std::path::PathBuf;

use crate::clients::ClientId;

use super::common::push_unique_scan_task;
use super::ScanPlan;

pub(super) fn add_default_tasks(plan: &mut ScanPlan<'_>) {
    for client_id in &plan.enabled {
        if has_specialized_discovery(*client_id) {
            continue;
        }
        let path = client_id
            .data()
            .resolve_path_with_env_strategy(plan.home_dir, plan.use_env_roots);
        push_unique_scan_task(
            &mut plan.tasks,
            &mut plan.seen_scan_roots,
            *client_id,
            path,
            client_id.data().pattern,
        );
    }
}

pub(super) fn add_workbuddy_tasks(plan: &mut ScanPlan<'_>) {
    if plan.has(ClientId::WorkBuddy) {
        plan.push_with_pattern(
            ClientId::WorkBuddy,
            PathBuf::from(plan.home_dir).join(".workbuddy/projects"),
            "*.jsonl",
        );
    }
}

fn has_specialized_discovery(client_id: ClientId) -> bool {
    matches!(
        client_id,
        ClientId::OpenCode
            | ClientId::Codex
            | ClientId::OpenClaw
            | ClientId::RooCode
            | ClientId::KiloCode
            | ClientId::Cline
            | ClientId::Kilo
            | ClientId::Hermes
            | ClientId::Goose
            | ClientId::Zed
            | ClientId::Crush
            | ClientId::Codebuff
            | ClientId::Freebuff
            | ClientId::Kimi
            | ClientId::Gjc
            | ClientId::MiMoCode
            | ClientId::DevinCli
            | ClientId::Grok
            | ClientId::PrimeAgent
    )
}
