use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct AttachedThread {
    pub(super) account: AccountId,
    /// Connections accepted by the legacy single-process router. This field is
    /// retained so existing runtime files deserialize without inventing an
    /// owner for sockets that cannot survive the topology cutover.
    #[serde(default)]
    pub(super) connections: u32,
    /// Live connections accepted by restart-independent worker generations.
    #[serde(default)]
    pub(super) connection_owners: BTreeMap<u64, WorkerConnectionCount>,
}

impl AttachedThread {
    pub(super) fn connections(&self) -> u32 {
        self.connection_owners
            .values()
            .fold(self.connections, |total, owner| {
                total.saturating_add(owner.count)
            })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct WorkerConnectionCount {
    pub(super) instance_id: u64,
    pub(super) count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WorkerConnectionOwner {
    generation: u64,
    instance_id: u64,
}

impl WorkerConnectionOwner {
    pub(crate) fn new(generation: u64, instance_id: u64) -> Option<Self> {
        (generation != 0 && instance_id != 0).then_some(Self {
            generation,
            instance_id,
        })
    }

    pub(super) fn generation(self) -> u64 {
        self.generation
    }

    pub(super) fn instance_id(self) -> u64 {
        self.instance_id
    }
}
