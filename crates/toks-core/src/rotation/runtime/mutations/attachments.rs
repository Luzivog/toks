use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::super::{
    AttachedThread, RotationRuntime, ThreadAccountConflict, ThreadId, WorkerConnectionCount,
    WorkerConnectionOwner,
};

impl RotationRuntime {
    pub fn thread_attached(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<bool, ThreadAccountConflict> {
        self.thread_attached_for(None, account, thread)
    }

    pub(crate) fn thread_attached_by(
        &mut self,
        owner: WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<bool, ThreadAccountConflict> {
        self.thread_attached_for(Some(owner), account, thread)
    }

    fn thread_attached_for(
        &mut self,
        owner: Option<WorkerConnectionOwner>,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<bool, ThreadAccountConflict> {
        self.claim_thread_account(account, thread)?;
        let attachment_before = self.attached_threads.get(thread).cloned();
        let attachment = self
            .attached_threads
            .entry(thread.clone())
            .or_insert_with(|| AttachedThread {
                account: account.clone(),
                connections: 0,
                connection_owners: BTreeMap::new(),
            });
        match owner {
            Some(owner) => open_owned(attachment, owner),
            None => attachment.connections = attachment.connections.saturating_add(1),
        }
        let mut changed = self
            .accounts
            .get_mut(account)
            .is_some_and(|state| state.provisional_threads.remove(thread));
        changed |= self.release_reservation(account, thread);
        for (candidate, state) in &mut self.accounts {
            if candidate != account {
                changed |= state.grandfathered_threads.remove(thread);
                changed |= state.provisional_threads.remove(thread);
                changed |= state.thread_usage.remove(thread).is_some();
            }
        }
        Ok(changed | (self.attached_threads.get(thread) != attachment_before.as_ref()))
    }

    pub fn thread_detached(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        self.thread_detached_for(None, account, thread)
    }

    pub(crate) fn thread_detached_by(
        &mut self,
        owner: WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
    ) -> bool {
        self.thread_detached_for(Some(owner), account, thread)
    }

    fn thread_detached_for(
        &mut self,
        owner: Option<WorkerConnectionOwner>,
        account: &AccountId,
        thread: &ThreadId,
    ) -> bool {
        let Some(attachment) = self
            .attached_threads
            .get_mut(thread)
            .filter(|attachment| &attachment.account == account)
        else {
            return false;
        };
        let removed = match owner {
            Some(owner) => close_owned(attachment, owner),
            None => attachment
                .connections
                .checked_sub(1)
                .is_some_and(|remaining| {
                    attachment.connections = remaining;
                    true
                }),
        };
        if !removed {
            return false;
        }
        attachment
            .connection_owners
            .retain(|_, connections| connections.count > 0);
        if attachment.connections() != 0 {
            return true;
        }
        self.attached_threads.remove(thread);
        self.cancel_active_thread(account, thread);
        true
    }
}

fn open_owned(attachment: &mut AttachedThread, owner: WorkerConnectionOwner) {
    let connections = attachment
        .connection_owners
        .entry(owner.generation())
        .or_insert(WorkerConnectionCount {
            instance_id: owner.instance_id(),
            count: 0,
        });
    if connections.instance_id != owner.instance_id() {
        connections.instance_id = owner.instance_id();
        connections.count = 0;
    }
    connections.count = connections.count.saturating_add(1);
}

fn close_owned(attachment: &mut AttachedThread, owner: WorkerConnectionOwner) -> bool {
    attachment
        .connection_owners
        .get_mut(&owner.generation())
        .filter(|connections| connections.instance_id == owner.instance_id())
        .is_some_and(|connections| {
            let Some(remaining) = connections.count.checked_sub(1) else {
                return false;
            };
            connections.count = remaining;
            true
        })
}
