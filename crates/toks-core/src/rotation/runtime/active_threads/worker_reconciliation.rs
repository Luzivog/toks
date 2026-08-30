use std::collections::BTreeMap;

use super::{ActiveThread, ThreadAccountConflict};
use crate::rotation::runtime::{
    AttachedThread, RotationRuntime, UnixMillis, WorkerConnectionCount, WorkerConnectionInventory,
    WorkerConnectionOwner,
};

impl RotationRuntime {
    /// Replace this worker's persisted counts with its exact local inventory
    /// and publish locally pending continuation intent. Other generations,
    /// legacy ownerless counts, reservations, existing follow-up affinity, and
    /// request metadata remain untouched.
    pub(crate) fn reconcile_worker_connection_inventory(
        &mut self,
        owner: WorkerConnectionOwner,
        inventory: &WorkerConnectionInventory,
        at: UnixMillis,
    ) -> Result<bool, ThreadAccountConflict> {
        for ((account, thread), _) in inventory.iter() {
            self.claim_thread_account(account, thread)?;
        }

        let active_before = self.active_threads.clone();
        let attached_before = self.attached_threads.clone();

        for (thread, active) in &mut self.active_threads {
            let count = inventory.counts(&active.account_id, thread);
            set_owned_count(&mut active.stream_owners, owner, count.streams);
            if count.pending_follow_up {
                active.awaiting_follow_up = true;
                active.last_activity_at = at;
            }
        }
        for (thread, attached) in &mut self.attached_threads {
            let count = inventory.counts(&attached.account, thread);
            set_owned_count(&mut attached.connection_owners, owner, count.attachments);
        }

        for ((account, thread), count) in inventory.iter() {
            if count.streams > 0 || count.pending_follow_up {
                let active = self
                    .active_threads
                    .entry(thread.clone())
                    .or_insert_with(|| ActiveThread::new(account.clone(), at));
                set_owned_count(&mut active.stream_owners, owner, count.streams);
                if count.pending_follow_up {
                    active.awaiting_follow_up = true;
                    active.last_activity_at = at;
                }
            }
            if count.attachments > 0 {
                let attached = self
                    .attached_threads
                    .entry(thread.clone())
                    .or_insert_with(|| AttachedThread {
                        account: account.clone(),
                        connections: 0,
                        connection_owners: BTreeMap::new(),
                    });
                set_owned_count(&mut attached.connection_owners, owner, count.attachments);
            }
        }

        self.attached_threads
            .retain(|_, attached| attached.connections() > 0);
        self.active_threads.retain(|thread, active| {
            active.stream_count() > 0
                || active.reservations > 0
                || active.awaiting_follow_up
                || self.attached_threads.contains_key(thread)
        });

        Ok(self.active_threads != active_before || self.attached_threads != attached_before)
    }

    /// Clear only live transport ownership that no longer has a worker.
    /// Reservations, follow-up intent, and metadata backed by a surviving
    /// attachment are logical state, so they survive this reconciliation.
    pub(crate) fn reconcile_connection_owners(&mut self, surviving: &BTreeMap<u64, u64>) -> bool {
        let active_before = self.active_threads.clone();
        let attached_before = self.attached_threads.clone();
        for active in self.active_threads.values_mut() {
            active.streams = 0;
            active.stream_owners.retain(|generation, owner| {
                surviving.get(generation) == Some(&owner.instance_id) && owner.count > 0
            });
        }
        for attached in self.attached_threads.values_mut() {
            attached.connections = 0;
            attached.connection_owners.retain(|generation, owner| {
                surviving.get(generation) == Some(&owner.instance_id) && owner.count > 0
            });
        }
        self.attached_threads
            .retain(|_, attached| attached.connections() > 0);
        self.active_threads.retain(|thread, active| {
            active.stream_count() > 0
                || active.reservations > 0
                || active.awaiting_follow_up
                || self.attached_threads.contains_key(thread)
        });
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

fn set_owned_count(
    counts: &mut BTreeMap<u64, WorkerConnectionCount>,
    owner: WorkerConnectionOwner,
    desired: u32,
) {
    if desired == 0 {
        counts.remove(&owner.generation());
        return;
    }
    counts.insert(
        owner.generation(),
        WorkerConnectionCount {
            instance_id: owner.instance_id(),
            count: desired,
        },
    );
}
