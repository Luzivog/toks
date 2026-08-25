use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, ThreadOwnership};

mod queue;
mod thread_overrides;
#[cfg(test)]
mod thread_overrides_tests;

pub use thread_overrides::{InvalidThreadOverrideValue, ThreadOverride, ThreadOverrideChange};

pub(super) const SETTINGS_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationSettings {
    version: u8,
    enabled: bool,
    priority: Vec<AccountId>,
    excluded: BTreeSet<AccountId>,
    cancelled_threads: BTreeSet<ThreadId>,
    waiting_priority: Vec<ThreadId>,
    #[serde(default)]
    thread_overrides: BTreeMap<ThreadId, ThreadOverride>,
}

impl Default for RotationSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            enabled: false,
            priority: Vec::new(),
            excluded: BTreeSet::new(),
            cancelled_threads: BTreeSet::new(),
            waiting_priority: Vec::new(),
            thread_overrides: BTreeMap::new(),
        }
    }
}

impl RotationSettings {
    pub fn enabled(&self) -> bool {
        self.enabled
    }

    pub fn priority(&self) -> &[AccountId] {
        &self.priority
    }

    pub fn excluded(&self) -> &BTreeSet<AccountId> {
        &self.excluded
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.enabled, enabled) != enabled
    }

    pub fn set_included(&mut self, account: &AccountId, included: bool) -> bool {
        if included {
            self.excluded.remove(account)
        } else {
            self.excluded.insert(account.clone())
        }
    }

    pub fn move_to(&mut self, account: &AccountId, index: usize) -> bool {
        let Some(from) = self
            .priority
            .iter()
            .position(|candidate| candidate == account)
        else {
            return false;
        };
        let destination = index.min(self.priority.len().saturating_sub(1));
        if from == destination {
            return false;
        }
        let account = self.priority.remove(from);
        self.priority.insert(destination, account);
        true
    }

    /// Retain saved choices for known accounts and append newly discovered
    /// accounts in discovery order.
    pub fn reconcile(&mut self, discovered: &[AccountId]) -> bool {
        let before = self.clone();
        let known: BTreeSet<_> = discovered.iter().cloned().collect();
        let mut seen = BTreeSet::new();
        self.priority
            .retain(|account| known.contains(account) && seen.insert(account.clone()));
        for account in discovered {
            if seen.insert(account.clone()) {
                self.priority.push(account.clone());
            }
        }
        self.excluded.retain(|account| known.contains(account));
        *self != before
    }

    pub fn select_account(
        &self,
        runtime: &RotationRuntime,
        discovered: &[AccountId],
        now: super::UnixMillis,
    ) -> Option<AccountId> {
        if !self.enabled {
            return None;
        }
        let eligible = |account: &AccountId| {
            discovered.contains(account)
                && !self.excluded.contains(account)
                && runtime.is_available(account, now)
        };
        self.priority
            .iter()
            .find(|account| eligible(account))
            .cloned()
    }

    pub(crate) fn select_account_for_thread(
        &self,
        runtime: &RotationRuntime,
        discovered: &[AccountId],
        thread: &ThreadId,
        now: super::UnixMillis,
    ) -> Option<AccountId> {
        if !self.enabled {
            return None;
        }
        let eligible = |account: &AccountId| {
            discovered.contains(account)
                && !self.excluded.contains(account)
                && runtime.is_available(account, now)
        };
        match runtime.thread_ownership(thread) {
            ThreadOwnership::Owned(account) => (discovered.contains(&account)
                && !self.excluded.contains(&account)
                && (runtime.is_available(&account, now)
                    || runtime.can_drain(&account, thread, now)))
            .then_some(account),
            ThreadOwnership::Conflicting => None,
            ThreadOwnership::Unowned => runtime
                .draining_account(thread, now)
                .filter(|account| discovered.contains(account) && !self.excluded.contains(account))
                .or_else(|| {
                    self.priority
                        .iter()
                        .find(|account| eligible(account))
                        .cloned()
                }),
        }
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }

    pub(super) fn normalize(&mut self) {
        let mut seen = BTreeSet::new();
        self.priority.retain(|account| seen.insert(account.clone()));
        self.normalize_waiting();
        self.normalize_thread_overrides();
    }
}
