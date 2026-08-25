use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, ThreadRequestSettings, UnixMillis};

const ABANDONED_FOLLOW_UP_MILLIS: i64 = 24 * 60 * 60 * 1_000;

mod account_claim;
mod ownership;
mod reconciliation;
mod reservations;
mod worker_reconciliation;
mod workloads;

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
    pub(super) started_at: Option<UnixMillis>,
    pub(super) last_activity_at: UnixMillis,
    #[serde(default, skip_serializing_if = "ThreadRequestSettings::is_empty")]
    pub(super) request_settings: ThreadRequestSettings,
}

impl ActiveThread {
    pub(super) fn new(account_id: AccountId, at: UnixMillis) -> Self {
        Self {
            account_id,
            streams: 0,
            stream_owners: BTreeMap::new(),
            reservations: 0,
            awaiting_follow_up: false,
            started_at: Some(at),
            last_activity_at: at,
            request_settings: ThreadRequestSettings::default(),
        }
    }

    pub(super) fn is_live(&self) -> bool {
        self.stream_count() > 0 || self.reservations > 0
    }

    pub(super) fn reservations(&self) -> u32 {
        self.reservations
    }

    pub(super) fn awaiting_follow_up(&self) -> bool {
        self.awaiting_follow_up
    }
}

impl RotationRuntime {
    pub fn connection_opened(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> Result<(), ThreadAccountConflict> {
        self.connection_opened_for(None, account, thread, at, None)
    }

    pub(crate) fn connection_opened_observed(
        &mut self,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
        request_settings: ThreadRequestSettings,
    ) -> Result<(), ThreadAccountConflict> {
        self.connection_opened_for(None, account, thread, at, Some(request_settings))
    }

    pub(crate) fn connection_opened_by(
        &mut self,
        owner: super::WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
    ) -> Result<(), ThreadAccountConflict> {
        self.connection_opened_for(Some(owner), account, thread, at, None)
    }

    pub(crate) fn connection_opened_by_observed(
        &mut self,
        owner: super::WorkerConnectionOwner,
        account: &AccountId,
        thread: &ThreadId,
        at: UnixMillis,
        request_settings: ThreadRequestSettings,
    ) -> Result<(), ThreadAccountConflict> {
        self.connection_opened_for(Some(owner), account, thread, at, Some(request_settings))
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
}
