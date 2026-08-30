use std::collections::BTreeMap;

use anyhow::Result;

use super::Coordinator;
use crate::codex_router::proxy::RouterRuntimeHandle;

pub(super) fn connections(
    coordinator: &Coordinator,
    runtime: &RouterRuntimeHandle,
    previous: &mut Option<BTreeMap<u64, u64>>,
) -> Result<()> {
    let Some(current) = coordinator.reconcilable_worker_instances() else {
        return Ok(());
    };
    if previous.as_ref() == Some(&current) {
        return Ok(());
    }
    runtime.reconcile_connection_owners(&current)?;
    *previous = Some(current);
    Ok(())
}

pub(super) fn task_activity(
    coordinator: &Coordinator,
    runtime: &RouterRuntimeHandle,
    previous: &mut Option<BTreeMap<u64, u64>>,
) {
    let Some(current) = coordinator.reconcilable_worker_instances() else {
        return;
    };
    if previous.as_ref() == Some(&current) {
        return;
    }
    if runtime.reconcile_task_activity_owners(&current) {
        *previous = Some(current);
    }
}
