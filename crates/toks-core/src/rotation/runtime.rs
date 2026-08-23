use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationEvent, RotationEventKind, ThreadId, UnixMillis};

mod account;
mod active_threads;
mod mutations;
mod reconcile;

pub(super) const RUNTIME_VERSION: u8 = 1;
const EVENT_LIMIT: usize = 100;

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
    #[serde(default, rename = "quotaExhaustion")]
    quota_drain: Option<QuotaDrainState>,
    #[serde(default)]
    grandfathered_threads: BTreeSet<ThreadId>,
    #[serde(default)]
    provisional_threads: BTreeSet<ThreadId>,
    #[serde(default)]
    thread_usage: BTreeMap<ThreadId, account::ThreadUsage>,
    needs_sign_in: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaDrainState {
    until: UnixMillis,
    reset_known: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AttachedThread {
    account: AccountId,
    connections: u32,
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
pub struct WaitingThread {
    pub thread_id: ThreadId,
    pub since: UnixMillis,
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
    #[serde(skip)]
    attached_threads: BTreeMap<ThreadId, AttachedThread>,
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
            .filter(|thread| &thread.account_id == account)
            .count()
            .try_into()
            .unwrap_or(u32::MAX)
    }

    pub fn is_available(&self, account: &AccountId, now: UnixMillis) -> bool {
        self.accounts
            .get(account)
            .is_none_or(|state| state.availability(now) == AccountAvailability::Available)
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

    pub(super) fn push_event(&mut self, at: UnixMillis, event: RotationEventKind) {
        self.events.push_front(RotationEvent { at, event });
        self.events.truncate(EVENT_LIMIT);
    }

    pub(super) fn normalize(&mut self) {
        let mut seen = BTreeSet::new();
        self.waiting_threads
            .retain(|waiting| seen.insert(waiting.thread_id.clone()));
        for state in self.accounts.values_mut() {
            if state.quota_drain.is_none() && !state.block_confirmed {
                state.grandfathered_threads.clear();
                state.provisional_threads.clear();
                state.thread_usage.clear();
            }
        }
        self.attached_threads.clear();
        self.events.truncate(EVENT_LIMIT);
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }
}
