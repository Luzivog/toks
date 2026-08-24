use serde::{Deserialize, Serialize};
use std::num::NonZeroU64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct GenerationId(pub u64);

impl GenerationId {
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub(crate) struct WorkerInstanceId(NonZeroU64);

impl WorkerInstanceId {
    pub const fn new(raw: u64) -> Option<Self> {
        match NonZeroU64::new(raw) {
            Some(id) => Some(Self(id)),
            None => None,
        }
    }

    pub const fn raw(self) -> u64 {
        self.0.get()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HandoffId {
    coordinator_epoch: u64,
    sequence: u64,
}

impl HandoffId {
    pub const fn new(coordinator_epoch: u64, sequence: u64) -> Self {
        Self {
            coordinator_epoch,
            sequence,
        }
    }

    pub const fn coordinator_epoch(self) -> u64 {
        self.coordinator_epoch
    }

    pub const fn sequence(self) -> u64 {
        self.sequence
    }
}
