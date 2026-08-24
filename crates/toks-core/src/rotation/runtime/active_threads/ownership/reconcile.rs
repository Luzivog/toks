use std::collections::BTreeMap;

use super::super::super::{RotationRuntime, WorkerConnectionOwner};

impl RotationRuntime {
    /// Clear only live transport ownership that no longer has a worker.
    /// Reservations and follow-up intent are logical task state, not socket
    /// ownership, so they deliberately survive this reconciliation.
    pub(crate) fn reconcile_connection_owners(&mut self, surviving: &BTreeMap<u64, u64>) -> bool {
        let active_before = self.active_threads.clone();
        let attached_before = self.attached_threads.clone();
        for active in self.active_threads.values_mut() {
            active.streams = 0;
            active.stream_owners.retain(|generation, owner| {
                surviving.get(generation) == Some(&owner.instance_id) && owner.count > 0
            });
        }
        self.active_threads.retain(|_, active| {
            active.stream_count() > 0 || active.reservations > 0 || active.awaiting_follow_up
        });
        for attached in self.attached_threads.values_mut() {
            attached.connections = 0;
            attached.connection_owners.retain(|generation, owner| {
                surviving.get(generation) == Some(&owner.instance_id) && owner.count > 0
            });
        }
        self.attached_threads
            .retain(|_, attached| attached.connections() > 0);
        self.active_threads != active_before || self.attached_threads != attached_before
    }

    /// Register the process now serving one systemd generation. Reusing a
    /// generation number after a worker crash cannot inherit the dead
    /// process's socket counts.
    pub(crate) fn adopt_worker_instance(&mut self, owner: WorkerConnectionOwner) -> bool {
        let mut surviving = self
            .active_threads
            .values()
            .flat_map(|active| {
                active
                    .stream_owners
                    .iter()
                    .map(|(generation, streams)| (*generation, streams.instance_id))
            })
            .chain(self.attached_threads.values().flat_map(|attached| {
                attached
                    .connection_owners
                    .iter()
                    .map(|(generation, connections)| (*generation, connections.instance_id))
            }))
            .collect::<BTreeMap<_, _>>();
        surviving.insert(owner.generation(), owner.instance_id());
        self.reconcile_connection_owners(&surviving)
    }
}
