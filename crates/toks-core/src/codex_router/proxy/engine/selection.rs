use std::collections::BTreeSet;

use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::{RotationEventKind, RotationRuntime, RotationSettings, ThreadId};

use super::super::types::{CredentialFailure, RouteCredential};
use super::{now, Engine, RouteTier};

impl Engine {
    pub async fn select_for_thread(
        &self,
        thread: Option<&ThreadId>,
        skipped: &BTreeSet<AccountId>,
    ) -> Result<Option<RouteCredential>> {
        loop {
            let Some(account) = self.eligible_account_except(thread, skipped, true)? else {
                return Ok(None);
            };
            match self.credentials.credential(&account).await {
                Ok(credential) => return Ok(Some(credential)),
                Err(CredentialFailure::NeedsSignIn) => {
                    self.release_selected(thread, &account)?;
                    self.auth_failed(&account)?;
                }
                Err(CredentialFailure::Temporary(error)) => {
                    self.release_selected(thread, &account)?;
                    return Err(error);
                }
            }
        }
    }

    pub fn eligible_account(&self) -> Result<Option<AccountId>> {
        self.eligible_account_except(None, &BTreeSet::new(), false)
    }

    pub fn eligible_account_for_thread(&self, thread: &ThreadId) -> Result<Option<AccountId>> {
        self.eligible_account_except(Some(thread), &BTreeSet::new(), false)
    }

    pub fn attach(&self, account: &AccountId, thread: &ThreadId) -> Result<bool> {
        let mut settings = self.settings.load()?;
        let discovered = self.credentials.account_ids();
        settings.reconcile(&discovered);
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        let before = runtime.clone();
        let selected = selected_account(
            &settings,
            &runtime,
            &discovered,
            Some(thread),
            &BTreeSet::new(),
        );
        if selected.as_ref() != Some(account) {
            let changed = runtime.release_reservation(account, thread);
            if changed {
                if let Err(error) = self.runtime_store.save(&runtime) {
                    *runtime = before;
                    return Err(error);
                }
            }
            return Ok(false);
        }
        let changed = runtime.thread_attached(account, thread);
        if changed {
            if let Err(error) = self.runtime_store.save(&runtime) {
                *runtime = before;
                return Err(error);
            }
        }
        Ok(true)
    }

    pub fn route(&self, account: &AccountId, thread: &ThreadId) -> Result<Option<RouteTier>> {
        let mut settings = self.settings.load()?;
        let discovered = self.credentials.account_ids();
        settings.reconcile(&discovered);
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        let before = runtime.clone();
        let selected = selected_account(
            &settings,
            &runtime,
            &discovered,
            Some(thread),
            &BTreeSet::new(),
        );
        if selected.as_ref() != Some(account) {
            let changed = runtime.release_reservation(account, thread);
            if changed {
                if let Err(error) = self.runtime_store.save(&runtime) {
                    *runtime = before;
                    return Err(error);
                }
            }
            return Ok(None);
        }
        let at = now();
        let tier = if runtime.can_drain(account, thread, at) {
            if runtime.requires_standard_tier(account, thread, at) {
                RouteTier::Standard
            } else {
                RouteTier::Fast
            }
        } else {
            RouteTier::Original
        };
        let previous = runtime
            .events()
            .iter()
            .find_map(|event| match &event.event {
                RotationEventKind::Routed {
                    thread_id,
                    account_id,
                } if thread_id == thread => Some(account_id.clone()),
                _ => None,
            });
        if let Some(previous) = previous.filter(|previous| previous != account) {
            runtime.rotated(thread, &previous, account, at);
        }
        runtime.resumed(thread, account, at);
        runtime.connection_opened(account, thread, at);
        if let Err(error) = self.runtime_store.save(&runtime) {
            *runtime = before;
            return Err(error);
        }
        Ok(Some(tier))
    }

    fn eligible_account_except(
        &self,
        thread: Option<&ThreadId>,
        skipped: &BTreeSet<AccountId>,
        reserve: bool,
    ) -> Result<Option<AccountId>> {
        let mut settings = self.settings.load()?;
        let discovered = self.credentials.account_ids();
        // The UI owns persisted settings. Reconcile only in memory so a newly
        // enrolled account can route before the next UI poll.
        settings.reconcile(&discovered);
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        let selected = selected_account(&settings, &runtime, &discovered, thread, skipped);
        if reserve {
            if let Some((thread, account)) = thread.zip(selected.as_ref()) {
                let before = runtime.clone();
                runtime.reserve_thread(account, thread, now());
                if let Err(error) = self.runtime_store.save(&runtime) {
                    *runtime = before;
                    return Err(error);
                }
            }
        }
        Ok(selected)
    }

    fn release_selected(&self, thread: Option<&ThreadId>, account: &AccountId) -> Result<()> {
        if let Some(thread) = thread {
            self.release_reservation(account, thread)?;
        }
        Ok(())
    }
}

fn selected_account(
    settings: &RotationSettings,
    runtime: &RotationRuntime,
    discovered: &[AccountId],
    thread: Option<&ThreadId>,
    skipped: &BTreeSet<AccountId>,
) -> Option<AccountId> {
    if !settings.enabled() {
        return None;
    }
    thread
        .and_then(|thread| runtime.draining_account(thread, now()))
        .filter(|account| {
            discovered.contains(account)
                && !settings.excluded().contains(account)
                && !skipped.contains(account)
        })
        .or_else(|| {
            settings
                .priority()
                .iter()
                .find(|account| {
                    discovered.contains(account)
                        && !settings.excluded().contains(account)
                        && !skipped.contains(account)
                        && runtime.is_available(account, now())
                })
                .cloned()
        })
}
