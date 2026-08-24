use std::collections::BTreeMap;
use std::time::Duration;

use tokio::time::Instant;

use crate::codex_router::host::GenerationId;

const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum WaitKey {
    WorkerReady(GenerationId),
    AdmissionsPaused(GenerationId),
    TargetAccepting(GenerationId),
    AdmissionsResumed(GenerationId),
}

#[derive(Default)]
pub(super) struct DeploymentWait {
    waits: BTreeMap<WaitKey, Instant>,
}

impl DeploymentWait {
    pub(super) fn arm(&mut self, key: WaitKey, now: Instant) {
        self.waits.entry(key).or_insert(now);
    }

    pub(super) fn is_armed(&self, key: WaitKey) -> bool {
        self.waits.contains_key(&key)
    }

    pub(super) fn acknowledge(&mut self, key: WaitKey) {
        self.waits.remove(&key);
    }

    pub(super) fn clear_generation(&mut self, generation: GenerationId) {
        self.waits.retain(|key, _| match key {
            WaitKey::WorkerReady(found)
            | WaitKey::AdmissionsPaused(found)
            | WaitKey::TargetAccepting(found)
            | WaitKey::AdmissionsResumed(found) => *found != generation,
        });
    }

    pub(super) fn take_expired(&mut self, now: Instant) -> Vec<WaitKey> {
        let expired = self
            .waits
            .iter()
            .filter_map(|(key, armed)| {
                (now.duration_since(*armed) >= ACTIVATION_TIMEOUT).then_some(*key)
            })
            .collect::<Vec<_>>();
        for key in &expired {
            self.waits.remove(key);
        }
        expired
    }
}
