use std::collections::BTreeSet;

use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::types::{CredentialFailure, RouteCredential};
use super::{now, Engine};

impl Engine {
    pub async fn select(&self, skipped: &BTreeSet<AccountId>) -> Result<Option<RouteCredential>> {
        self.select_for_thread(None, skipped).await
    }

    pub async fn select_for_thread(
        &self,
        thread: Option<&ThreadId>,
        skipped: &BTreeSet<AccountId>,
    ) -> Result<Option<RouteCredential>> {
        loop {
            let Some(account) = self.eligible_account_except(thread, skipped)? else {
                return Ok(None);
            };
            match self.credentials.credential(&account).await {
                Ok(credential) => return Ok(Some(credential)),
                Err(CredentialFailure::NeedsSignIn) => self.auth_failed(&account)?,
                Err(CredentialFailure::Temporary(error)) => return Err(error),
            }
        }
    }

    pub fn eligible_account(&self) -> Result<Option<AccountId>> {
        self.eligible_account_except(None, &BTreeSet::new())
    }

    fn eligible_account_except(
        &self,
        thread: Option<&ThreadId>,
        skipped: &BTreeSet<AccountId>,
    ) -> Result<Option<AccountId>> {
        let mut settings = self.settings.load()?;
        let discovered = self.credentials.account_ids();
        // The UI owns persisted settings. Reconcile only in memory so a newly
        // enrolled account can route before the next UI poll.
        settings.reconcile(&discovered);
        let runtime = self.runtime.lock().expect("router runtime poisoned");
        if !settings.enabled() {
            return Ok(None);
        }
        if settings.fast_when_draining() {
            if let Some(account) = thread
                .and_then(|thread| runtime.draining_account(thread, now()))
                .filter(|account| {
                    discovered.contains(account)
                        && !settings.excluded().contains(account)
                        && !skipped.contains(account)
                })
            {
                return Ok(Some(account));
            }
        }
        Ok(settings
            .priority()
            .iter()
            .find(|account| {
                discovered.contains(account)
                    && !settings.excluded().contains(account)
                    && !skipped.contains(account)
                    && runtime.is_available(account, now())
            })
            .cloned())
    }
}
