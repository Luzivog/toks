use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId};

mod queue;

pub(super) const SETTINGS_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RotationSettings {
    version: u8,
    enabled: bool,
    priority: Vec<AccountId>,
    excluded: BTreeSet<AccountId>,
    preferred: Option<AccountId>,
    cancelled_threads: BTreeSet<ThreadId>,
    waiting_priority: Vec<ThreadId>,
}

impl Default for RotationSettings {
    fn default() -> Self {
        Self {
            version: SETTINGS_VERSION,
            enabled: false,
            priority: Vec::new(),
            excluded: BTreeSet::new(),
            preferred: None,
            cancelled_threads: BTreeSet::new(),
            waiting_priority: Vec::new(),
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

    pub fn preferred(&self) -> Option<&AccountId> {
        self.preferred.as_ref()
    }

    pub fn set_enabled(&mut self, enabled: bool) -> bool {
        std::mem::replace(&mut self.enabled, enabled) != enabled
    }

    pub fn set_included(&mut self, account: &AccountId, included: bool) -> bool {
        if included {
            self.excluded.remove(account)
        } else {
            let changed = self.excluded.insert(account.clone());
            if self.preferred.as_ref() == Some(account) {
                self.preferred = None;
            }
            changed
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

    pub fn use_now(&mut self, account: &AccountId) -> bool {
        if !self.priority.contains(account) || self.excluded.contains(account) {
            return false;
        }
        if self.preferred.as_ref() == Some(account) {
            return false;
        }
        self.preferred = Some(account.clone());
        true
    }

    pub fn clear_preferred(&mut self) -> bool {
        self.preferred.take().is_some()
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
        if self
            .preferred
            .as_ref()
            .is_some_and(|id| !known.contains(id))
        {
            self.preferred = None;
        }
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
        self.preferred
            .iter()
            .chain(self.priority.iter())
            .find(|account| eligible(account))
            .cloned()
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }

    pub(super) fn normalize(&mut self) {
        let mut seen = BTreeSet::new();
        self.priority.retain(|account| seen.insert(account.clone()));
        self.normalize_waiting();
        if self
            .preferred
            .as_ref()
            .is_some_and(|account| self.excluded.contains(account))
        {
            self.preferred = None;
        }
    }
}
