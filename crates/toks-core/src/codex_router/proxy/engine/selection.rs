use std::collections::BTreeSet;

use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, UnixMillis};
use crate::storage::StoreUpdate;

use super::Engine;
use crate::codex_router::proxy::headers::ResumeMarker;
use crate::codex_router::proxy::types::{CredentialFailure, RouteCredential};

mod activation;
mod claims;
mod policy;
mod quarantine;
mod repair;
use policy::selected_account;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::codex_router::proxy) enum RouteSelection<T> {
    Selected(T),
    ResumeDenied,
    Unavailable,
}

impl<T> RouteSelection<T> {
    fn selected(self) -> Option<T> {
        match self {
            Self::Selected(value) => Some(value),
            Self::ResumeDenied | Self::Unavailable => None,
        }
    }
}

impl Engine {
    fn auth_failed(&self, account: &AccountId, rejected: Option<&RouteCredential>) -> Result<()> {
        self.runtime.update(|runtime| {
            runtime.auth_failed_for_credential(
                account,
                UnixMillis::now(),
                rejected.map(RouteCredential::fingerprint).as_deref(),
            );
            StoreUpdate::Changed(())
        })
    }

    pub fn permanent_auth_failure(&self, rejected: &RouteCredential) -> Result<()> {
        self.auth_failed(&rejected.account_id, Some(rejected))
    }

    #[cfg(test)]
    pub async fn select_for_thread(
        &self,
        thread: Option<&ThreadId>,
        skipped: &BTreeSet<AccountId>,
    ) -> Result<Option<RouteCredential>> {
        Ok(self
            .select_for_thread_authorized(thread, ResumeMarker::Absent, skipped)
            .await?
            .selected())
    }

    pub async fn select_for_thread_authorized(
        &self,
        thread: Option<&ThreadId>,
        marker: ResumeMarker<'_>,
        skipped: &BTreeSet<AccountId>,
    ) -> Result<RouteSelection<RouteCredential>> {
        let mut repaired = self.repaired_credentials().await?;
        loop {
            let account = match self.eligible_account_except(thread, marker, skipped, true)? {
                RouteSelection::Selected(account) => account,
                RouteSelection::ResumeDenied => return Ok(RouteSelection::ResumeDenied),
                RouteSelection::Unavailable => return Ok(RouteSelection::Unavailable),
            };
            let credential = match repaired.remove(&account) {
                Some(credential) => Ok(credential),
                None => self.credentials.credential(&account).await,
            };
            match credential {
                Ok(credential) => {
                    let credential = match verified_credential(&account, credential) {
                        Ok(credential) => credential,
                        Err(error) => {
                            self.release_selected(thread, &account)?;
                            return Err(error);
                        }
                    };
                    if self.credential_was_rejected(&credential)? {
                        self.release_selected(thread, &account)?;
                        self.auth_failed(&account, Some(&credential))?;
                        continue;
                    }
                    return Ok(RouteSelection::Selected(credential));
                }
                Err(CredentialFailure::NeedsSignIn) => {
                    self.release_selected(thread, &account)?;
                    self.auth_failed(&account, None)?;
                }
                Err(CredentialFailure::Temporary(error)) => {
                    self.release_selected(thread, &account)?;
                    return Err(error);
                }
            }
        }
    }

    pub fn eligible_account(&self) -> Result<Option<AccountId>> {
        Ok(self
            .eligible_account_except(None, ResumeMarker::Absent, &BTreeSet::new(), false)?
            .selected())
    }

    pub fn eligible_account_for_thread(&self, thread: &ThreadId) -> Result<Option<AccountId>> {
        Ok(self
            .eligible_account_except(Some(thread), ResumeMarker::Absent, &BTreeSet::new(), false)?
            .selected())
    }

    pub fn eligible_account_for_thread_authorized(
        &self,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> Result<RouteSelection<AccountId>> {
        self.eligible_account_except(
            Some(thread),
            ResumeMarker::from_attempt(resume_attempt),
            &BTreeSet::new(),
            false,
        )
    }

    fn eligible_account_except(
        &self,
        thread: Option<&ThreadId>,
        marker: ResumeMarker<'_>,
        skipped: &BTreeSet<AccountId>,
        reserve: bool,
    ) -> Result<RouteSelection<AccountId>> {
        let discovered = self.credentials.account_ids();
        self.settings.update(|settings| {
            // The UI owns persisted settings. Reconcile only in memory so a
            // newly enrolled account can route before the next UI poll.
            settings.reconcile(&discovered);
            let selected = self.runtime.update(|runtime| {
                let at = UnixMillis::now();
                let selected =
                    selected_account(settings, runtime, &discovered, thread, marker, skipped, at);
                let mut changed = false;
                if reserve {
                    if let (Some(thread), RouteSelection::Selected(account)) = (thread, &selected) {
                        if runtime.reserve_thread(account, thread, at).is_ok() {
                            changed = true;
                        } else {
                            let denied = if marker.is_present() {
                                RouteSelection::ResumeDenied
                            } else {
                                RouteSelection::Unavailable
                            };
                            return StoreUpdate::Unchanged(denied);
                        }
                    }
                }
                StoreUpdate::from_changed(selected, changed)
            });
            StoreUpdate::Unchanged(selected)
        })?
    }

    fn release_selected(&self, thread: Option<&ThreadId>, account: &AccountId) -> Result<()> {
        if let Some(thread) = thread {
            self.release_reservation(account, thread)?;
        }
        Ok(())
    }
}

fn verified_credential(
    account: &AccountId,
    credential: RouteCredential,
) -> Result<RouteCredential> {
    anyhow::ensure!(
        credential.account_id == *account,
        "credential source returned account {} for requested account {}",
        credential.account_id,
        account
    );
    Ok(credential)
}
