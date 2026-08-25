use std::collections::BTreeMap;

use super::{RotationRuntime, UnixMillis};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct GenerationWorkload {
    pub(crate) task_count: u32,
    pub(crate) oldest_task_at: Option<UnixMillis>,
}

impl RotationRuntime {
    pub(crate) fn generation_workloads(&self) -> BTreeMap<u64, GenerationWorkload> {
        let mut workloads = BTreeMap::<u64, GenerationWorkload>::new();
        for active in self.active_threads.values() {
            let started_at = active.started_at.unwrap_or(active.last_activity_at);
            for generation in active.stream_owners.keys() {
                let workload = workloads.entry(*generation).or_default();
                workload.task_count = workload.task_count.saturating_add(1);
                workload.oldest_task_at = Some(
                    workload
                        .oldest_task_at
                        .map_or(started_at, |oldest| oldest.min(started_at)),
                );
            }
        }
        workloads
    }
}
