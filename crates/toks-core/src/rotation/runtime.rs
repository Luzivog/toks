use std::collections::{BTreeMap, BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationEvent, RotationEventKind, ThreadId, UnixMillis};

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
    needs_sign_in: bool,
    active_streams: u32,
}

impl AccountRuntime {
    pub fn blocked_until(&self) -> Option<UnixMillis> {
        self.blocked_until
    }

    pub fn needs_sign_in(&self) -> bool {
        self.needs_sign_in
    }

    pub fn active_streams(&self) -> u32 {
        self.active_streams
    }
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
        self.accounts.get(account).is_none_or(|state| {
            !state.needs_sign_in && state.blocked_until.is_none_or(|until| until <= now)
        })
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
            }
        }
        self.accounts != before
    }

    pub(super) fn push_event(&mut self, at: UnixMillis, event: RotationEventKind) {
        self.events.push_front(RotationEvent { at, event });
        self.events.truncate(EVENT_LIMIT);
    }

    pub(super) fn normalize(&mut self) {
        let mut seen = BTreeSet::new();
        self.waiting_threads
            .retain(|waiting| seen.insert(waiting.thread_id.clone()));
        self.events.truncate(EVENT_LIMIT);
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }
}
