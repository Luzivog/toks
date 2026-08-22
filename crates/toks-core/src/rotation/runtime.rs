use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationEvent, RotationEventKind, ThreadId, UnixMillis};

mod account;
mod active_threads;
mod mutations;

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
    #[serde(default)]
    quota_exhaustion: Option<QuotaExhaustionState>,
    #[serde(default)]
    grandfathered_threads: BTreeSet<ThreadId>,
    needs_sign_in: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QuotaExhaustionState {
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

    pub fn draining_account(&self, thread: &ThreadId, now: UnixMillis) -> Option<AccountId> {
        self.accounts
            .iter()
            .find(|(_, state)| state.can_drain(thread, now))
            .map(|(account, _)| account.clone())
    }

    /// Drop vanished accounts, create state for new accounts, and clear
    /// elapsed blocks. This mutation never touches waiting threads.
    pub fn reconcile(&mut self, discovered: &[AccountId], now: UnixMillis) -> bool {
        let before = self.accounts.clone();
        let known: BTreeSet<_> = discovered.iter().cloned().collect();
        self.accounts.retain(|account, _| known.contains(account));
        for account in discovered {
            let state = self.accounts.entry(account.clone()).or_default();
            if state.blocked_until.is_some_and(|until| until <= now) {
                state.blocked_until = None;
                state.block_confirmed = false;
                state.block_reset_known = false;
            }
        }
        self.accounts != before || self.reconcile_active_threads(&known, now)
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
            if state.quota_exhaustion.is_none() {
                state.grandfathered_threads.clear();
            }
        }
        self.attached_threads.clear();
        self.events.truncate(EVENT_LIMIT);
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }
}
