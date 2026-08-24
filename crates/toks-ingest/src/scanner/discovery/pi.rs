use crate::clients::ClientId;

use super::common::join_native;
use super::ScanPlan;

pub(super) fn add_tasks(plan: &mut ScanPlan<'_>) {
    if plan.has(ClientId::Pi) {
        plan.push(
            ClientId::Pi,
            join_native(plan.home_dir, ".omp/agent/sessions"),
        );
    }
}
