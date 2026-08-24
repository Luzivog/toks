use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, UnixMillis};

const ABANDONED_FOLLOW_UP_MILLIS: i64 = 24 * 60 * 60 * 1_000;

mod account_claim;
mod ownership;
mod reservations;

pub use account_claim::ThreadAccountConflict;
pub(crate) use account_claim::ThreadOwnership;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ActiveThread {
    pub(super) account_id: AccountId,
    /// Streams owned by the legacy single-process router. New workers record
    /// ownership in `stream_owners`; keeping this scalar preserves old files.
    #[serde(default)]
    streams: u32,
    #[serde(default)]
    stream_owners: BTreeMap<u64, super::WorkerConnectionCount>,
    #[serde(default)]
    reservations: u32,
    awaiting_follow_up: bool,
    #[serde(default)]
    started_at: Option<UnixMillis>,
    last_activity_at: UnixMillis,
}

impl ActiveThread {
    pub(super) fn is_live(&self) -> bool {
        self.stream_count() > 0 || self.reservations > 0
    }
}

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

    pub fn connection_opened(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> Result<(), ThreadAccountConflict> {
        self.connection_opened_for(None, account, thread, at)
    }

    pub(crate) fn connection_opened_by(
        &mut self,
        owner: super::WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> Result<(), ThreadAccountConflict> {
        self.connection_opened_for(Some(owner), account, thread, at)
    }

    pub fn connection_closed(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        self.connection_closed_for(None, account, thread, at)
    }

    pub(crate) fn connection_closed_by(
        &mut self,
        owner: super::WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        self.connection_closed_for(Some(owner), account, thread, at)
    }

    pub fn connection_continues(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        self.connection_continues_for(None, account, thread, at)
    }
    pub(crate) fn connection_continues_by(
        &mut self,
        owner: super::WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> bool {
        self.connection_continues_for(Some(owner), account, thread, at)
    }

    pub fn reset_connections(&mut self, _at: UnixMillis) -> bool {
        let changed = !self.attached_threads.is_empty() || !self.active_threads.is_empty();
        let changed = self.clear_abandoned_reservations() | changed;
        self.attached_threads.clear();
        self.active_threads.retain(|_, active| {
            if !active.awaiting_follow_up {
                return false;
            }
            active.streams = 0;
            active.stream_owners.clear();
            active.reservations = 0;
            true
        });
        changed
    }

    pub(super) fn cancel_active_thread(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        if self.active_threads.get(thread).is_some_and(|active| {
            &active.account_id == account
                && active.stream_count() == 0
                && active.reservations == 0
                && !active.awaiting_follow_up
        }) {
            self.active_threads.remove(thread);
            true
        } else {
            false
        }
    }

    pub(super) fn reconcile_active_threads(
        &mut self,
        known: &BTreeSet<AccountId>,
        now: UnixMillis,
    ) -> bool {
        let before = self.active_threads.clone();
        let reservations_changed = self.expire_reservations(known, now);
        self.active_threads.retain(|_, thread| {
            thread.stream_count() > 0
                || thread.reservations > 0
                || (thread.awaiting_follow_up
                    && (!known.contains(&thread.account_id)
                        || thread
                            .last_activity_at
                            .get()
                            .saturating_add(ABANDONED_FOLLOW_UP_MILLIS)
                            > now.get()))
        });
        reservations_changed | (self.active_threads != before)
    }
}
