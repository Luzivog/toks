use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationEvent, RotationEventKind, ThreadId, UnixMillis};

mod account;
mod account_auth;
#[cfg(test)]
mod account_auth_tests;
mod active_threads;
mod connection_owner;
mod events;
mod mutations;
mod reconcile;
mod resume_admissions;
mod thread_rows;
#[cfg(test)]
mod thread_rows_tests;
mod validation;
mod waiting;

pub use active_threads::ThreadAccountConflict;
pub(crate) use active_threads::ThreadOwnership;
pub(crate) use connection_owner::WorkerConnectionOwner;
use connection_owner::{AttachedThread, WorkerConnectionCount};
pub(crate) use resume_admissions::{ResumeAuthorization, ResumeRoute, ResumeTerminal};
pub use thread_rows::{ThreadRequestSettings, ThreadRow, ThreadStatus};
pub use waiting::{WaitingId, WaitingThread};

pub(super) const RUNTIME_VERSION: u8 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RouterHealth {
    #[default]
    Unknown,
    Healthy,
    Failed,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountRuntime {
    blocked_until: Option<UnixMillis>,
    #[serde(default)]
    block_confirmed: bool,
    #[serde(default)]
    block_reset_known: bool,
    #[serde(default)]
    quota_authority_revision: u64,
    #[serde(default, rename = "quotaExhaustion")]
    quota_drain: Option<QuotaDrainState>,
    #[serde(default)]
    grandfathered_threads: BTreeSet<ThreadId>,
    #[serde(default)]
    provisional_threads: BTreeSet<ThreadId>,
    #[serde(default)]
    thread_usage: BTreeMap<ThreadId, account::ThreadUsage>,
    #[serde(flatten)]
    auth: account_auth::AccountAuthState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaDrainState {
    until: UnixMillis,
    reset_known: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountAvailability {
    Available,
    Draining {
        until: UnixMillis,
        reset_known: bool,
    },
    Blocked {
        until: UnixMillis,
        reset_known: bool,
    },
    NeedsSignIn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationRuntime {
    version: u8,
    health: RouterHealth,
    heartbeat_at: Option<UnixMillis>,
    accounts: BTreeMap<AccountId, AccountRuntime>,
    #[serde(default)]
    active_threads: BTreeMap<ThreadId, active_threads::ActiveThread>,
    #[serde(default)]
    attached_threads: BTreeMap<ThreadId, AttachedThread>,
    #[serde(default)]
    resume_admissions: BTreeMap<WaitingId, resume_admissions::ResumeAdmission>,
    waiting_threads: Vec<WaitingThread>,
    events: VecDeque<RotationEvent>,
}

impl Default for RotationRuntime {
    fn default() -> Self {
        Self {
            version: RUNTIME_VERSION,
            health: RouterHealth::Unknown,
            heartbeat_at: None,
            accounts: BTreeMap::new(),
            active_threads: BTreeMap::new(),
            attached_threads: BTreeMap::new(),
            resume_admissions: BTreeMap::new(),
            waiting_threads: Vec::new(),
            events: VecDeque::new(),
        }
    }
}

impl RotationRuntime {
    pub fn health(&self) -> RouterHealth {
        self.health
    }

    pub fn heartbeat_at(&self) -> Option<UnixMillis> {
        self.heartbeat_at
    }

    pub fn accounts(&self) -> &BTreeMap<AccountId, AccountRuntime> {
        &self.accounts
    }

    pub fn waiting_threads(&self) -> &[WaitingThread] {
        &self.waiting_threads
    }

    pub fn events(&self) -> &VecDeque<RotationEvent> {
        &self.events
    }

    pub fn active_threads(&self, account: &AccountId) -> u32 {
        self.active_threads
            .values()
            .filter(|thread| &thread.account_id == account && thread.is_live())
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    }

    pub fn is_available(&self, account: &AccountId, now: UnixMillis) -> bool {
        self.accounts
            .get(account)
            .is_none_or(|state| state.availability(now) == AccountAvailability::Available)
    }

    pub(crate) fn quota_authority_revision(&self, account: &AccountId) -> u64 {
        self.accounts
            .get(account)
            .map_or(0, |state| state.quota_authority_revision())
    }

    pub fn can_drain(&self, account: &AccountId, thread: &ThreadId, now: UnixMillis) -> bool {
        self.accounts
            .get(account)
            .is_some_and(|state| state.can_drain(thread, now))
    }

    pub fn requires_standard_tier(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        now: UnixMillis,
    ) -> bool {
        self.accounts
            .get(account)
            .is_some_and(|state| state.requires_standard_tier(thread, now))
    }

    pub fn draining_account(&self, thread: &ThreadId, now: UnixMillis) -> Option<AccountId> {
        self.accounts
            .iter()
            .find(|(_, state)| state.can_drain(thread, now))
            .map(|(account, _)| account.clone())
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }
}
