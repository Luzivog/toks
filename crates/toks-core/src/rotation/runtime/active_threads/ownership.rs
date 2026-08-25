use crate::accounts::AccountId;

use super::ActiveThread;
use crate::rotation::runtime::{
    RotationEventKind, RotationRuntime, ThreadAccountConflict, ThreadId, ThreadRequestSettings,
    UnixMillis, WorkerConnectionCount, WorkerConnectionOwner,
};

impl ActiveThread {
    pub(in crate::rotation::runtime) fn stream_count(&self) -> u32 {
        self.stream_owners
            .values()
            .fold(self.streams, |total, owner| {
                total.saturating_add(owner.count)
            })
    }

    fn open(&mut self, owner: Option<WorkerConnectionOwner>) {
        match owner {
            Some(owner) => {
                let streams =
                    self.stream_owners
                        .entry(owner.generation())
                        .or_insert(WorkerConnectionCount {
                            instance_id: owner.instance_id(),
                            count: 0,
                        });
                if streams.instance_id != owner.instance_id() {
                    streams.instance_id = owner.instance_id();
                    streams.count = 0;
                }
                streams.count = streams.count.saturating_add(1);
            }
            None => self.streams = self.streams.saturating_add(1),
        }
    }

    fn close(&mut self, owner: Option<WorkerConnectionOwner>) -> bool {
        let removed = match owner {
            Some(owner) => self
                .stream_owners
                .get_mut(&owner.generation())
                .filter(|streams| streams.instance_id == owner.instance_id())
                .is_some_and(|streams| {
                    let Some(remaining) = streams.count.checked_sub(1) else {
                        return false;
                    };
                    streams.count = remaining;
                    true
                }),
            None => self.streams.checked_sub(1).is_some_and(|remaining| {
                self.streams = remaining;
                true
            }),
        };
        self.stream_owners.retain(|_, streams| streams.count > 0);
        removed
    }
}

impl RotationRuntime {
    pub(super) fn connection_opened_for(
        &mut self,
        owner: Option<WorkerConnectionOwner>,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
        request_settings: Option<ThreadRequestSettings>,
    ) -> Result<(), ThreadAccountConflict> {
        self.claim_thread_account(account, thread)?;
        self.accounts.entry(account.clone()).or_default();
        if let Some(state) = self.accounts.get_mut(account) {
            state.provisional_threads.remove(thread);
        }
        let active = self
            .active_threads
            .entry(thread.clone())
            .or_insert_with(|| ActiveThread::new(account.clone(), at));
        active.reservations = active.reservations.saturating_sub(1);
        active.awaiting_follow_up = false;
        active.open(owner);
        active.last_activity_at = at;
        if let Some(request_settings) = request_settings {
            active.request_settings = request_settings;
        }
        self.push_event(
            at,
            RotationEventKind::Routed {
                thread_id: thread.clone(),
                account_id: account.clone(),
            },
        );
        Ok(())
    }

    pub(super) fn connection_closed_for(
        &mut self,
        owner: Option<WorkerConnectionOwner>,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        let attached = self
            .attached_threads
            .get(thread)
            .is_some_and(|attachment| attachment.connections() > 0);
        let Some(active) = self
            .active_threads
            .get_mut(thread)
            .filter(|active| &active.account_id == account)
        else {
            return false;
        };
        if !active.close(owner) {
            return false;
        }
        active.last_activity_at = at;
        if active.stream_count() == 0
            && active.reservations == 0
            && !active.awaiting_follow_up
            && !attached
        {
            self.active_threads.remove(thread);
        }
        true
    }

    pub(super) fn connection_continues_for(
        &mut self,
        owner: Option<WorkerConnectionOwner>,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        let Some(active) = self
            .active_threads
            .get_mut(thread)
            .filter(|active| &active.account_id == account)
        else {
            return false;
        };
        if !active.close(owner) {
            return false;
        }
        active.awaiting_follow_up = true;
        active.last_activity_at = at;
        true
    }
}
