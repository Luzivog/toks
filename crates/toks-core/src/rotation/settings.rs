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
    cancelled_threads: BTreeSet<ThreadId>,
    waiting_priority: Vec<ThreadId>,
    /// Serve threads already attached to a draining (0% remaining) account at
    /// the Fast service tier so the remaining work finishes quickly. Defaults to
    /// on, and defaults in for settings written before the field existed.
    #[serde(default = "enabled_by_default")]
    fast_when_draining: bool,
}

fn enabled_by_default() -> bool {
    true
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
            fast_when_draining: enabled_by_default(),
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

    pub fn fast_when_draining(&self) -> bool {
        self.fast_when_draining
    }

    pub fn set_fast_when_draining(&mut self, fast: bool) -> bool {
        std::mem::replace(&mut self.fast_when_draining, fast) != fast
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

    pub(super) fn version(&self) -> u8 {
        self.version
    }

    pub(super) fn normalize(&mut self) {
        let mut seen = BTreeSet::new();
        self.priority.retain(|account| seen.insert(account.clone()));
        self.normalize_waiting();
    }
}
