use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::ThreadId;

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

/// Exact live connections and unpublished continuation intent held by one
/// worker process.
///
/// This inventory is process-local and is never serialized. Workers publish
/// its aggregate counts and retry continuation intent through the existing
/// version-1 fields, so failed best-effort cleanup converges without changing
/// the runtime schema.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct WorkerConnectionInventory {
    threads: BTreeMap<(AccountId, ThreadId), WorkerInventoryCount>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(in crate::rotation::runtime) struct WorkerInventoryCount {
    pub(in crate::rotation::runtime) streams: u32,
    pub(in crate::rotation::runtime) attachments: u32,
    pub(in crate::rotation::runtime) pending_follow_up: bool,
}

impl WorkerConnectionInventory {
    pub(crate) fn stream_opened(&mut self, account: &AccountId, thread: &ThreadId) {
        let count = self.count_mut(account, thread);
        count.pending_follow_up = false;
        count.streams = count.streams.saturating_add(1);
    }

    pub(crate) fn stream_closed(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        self.close(account, thread, |count| &mut count.streams)
    }

    pub(crate) fn stream_continues(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        let key = (account.clone(), thread.clone());
        let Some(count) = self.threads.get_mut(&key) else {
            return false;
        };
        let Some(remaining) = count.streams.checked_sub(1) else {
            return false;
        };
        count.streams = remaining;
        count.pending_follow_up = true;
        true
    }

    pub(crate) fn continuation_published(&mut self, account: &AccountId, thread: &ThreadId) {
        let key = (account.clone(), thread.clone());
        if let Some(count) = self.threads.get_mut(&key) {
            count.pending_follow_up = false;
        }
        self.remove_empty(&key);
    }

    pub(crate) fn continuations_published(&mut self) {
        for count in self.threads.values_mut() {
            count.pending_follow_up = false;
        }
        self.threads.retain(|_, count| !count.is_empty());
    }

    pub(crate) fn attachment_opened(&mut self, account: &AccountId, thread: &ThreadId) {
        let count = self.count_mut(account, thread);
        count.attachments = count.attachments.saturating_add(1);
    }

    pub(crate) fn attachment_closed(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        self.close(account, thread, |count| &mut count.attachments)
    }

    pub(in crate::rotation::runtime) fn iter(
        &self,
    ) -> impl Iterator<Item = (&(AccountId, ThreadId), &WorkerInventoryCount)> {
        self.threads.iter()
    }

    pub(in crate::rotation::runtime) fn counts(
        &self,
        account: &AccountId,
        thread: &ThreadId,
    ) -> WorkerInventoryCount {
        self.threads
            .get(&(account.clone(), thread.clone()))
            .copied()
            .unwrap_or_default()
    }

    fn count_mut(&mut self, account: &AccountId, thread: &ThreadId) -> &mut WorkerInventoryCount {
        self.threads
            .entry((account.clone(), thread.clone()))
            .or_default()
    }

    fn close(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        select: impl FnOnce(&mut WorkerInventoryCount) -> &mut u32,
    ) -> bool {
        let key = (account.clone(), thread.clone());
        let Some(count) = self.threads.get_mut(&key) else {
            return false;
        };
        let selected = select(count);
        let Some(remaining) = selected.checked_sub(1) else {
            return false;
        };
        *selected = remaining;
        self.remove_empty(&key);
        true
    }

    fn remove_empty(&mut self, key: &(AccountId, ThreadId)) {
        if self
            .threads
            .get(key)
            .is_some_and(WorkerInventoryCount::is_empty)
        {
            self.threads.remove(key);
        }
    }
}

impl WorkerInventoryCount {
    fn is_empty(&self) -> bool {
        self.streams == 0 && self.attachments == 0 && !self.pending_follow_up
    }
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
