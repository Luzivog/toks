use anyhow::Result;

use crate::accounts::AccountId;
use crate::codex_router::account_activation::RouteClaim;
use crate::codex_router::proxy::headers::ActivationMarker;
use crate::codex_router::proxy::types::{CredentialFailure, RouteCredential};
use crate::rotation::{ThreadId, UnixMillis};
use crate::storage::StoreUpdate;

use super::{verified_credential, Engine};

impl Engine {
    pub async fn select_for_activation_thread(
        &self,
        thread: &ThreadId,
        marker: ActivationMarker<'_>,
    ) -> Result<Option<RouteCredential>> {
        let Some(attempt) = marker.attempt() else {
            return Ok(None);
        };
        let mut repaired = self.repaired_credentials().await?;
        let RouteClaim::Selected(account) =
            self.activation
                .claim_route(attempt, thread, UnixMillis::now().get())?
        else {
            return Ok(None);
        };
        if !self.reserve_activation_account(&account, thread)? {
            return Ok(None);
        }
        let credential = match repaired.remove(&account) {
            Some(credential) => Ok(credential),
            None => self.credentials.credential(&account).await,
        };
        let credential = match credential {
            Ok(credential) => match verified_credential(&account, credential) {
                Ok(credential) => credential,
                Err(error) => {
                    self.release_reservation(&account, thread)?;
                    return Err(error);
                }
            },
            Err(CredentialFailure::NeedsSignIn) => {
                self.release_reservation(&account, thread)?;
                self.auth_failed(&account, None)?;
                return Ok(None);
            }
            Err(CredentialFailure::Temporary(error)) => {
                self.release_reservation(&account, thread)?;
                return Err(error);
            }
        };
        let rejected = match self.credential_was_rejected(&credential) {
            Ok(rejected) => rejected,
            Err(error) => {
                self.release_reservation(&account, thread)?;
                return Err(error);
            }
        };
        if rejected {
            self.release_reservation(&account, thread)?;
            self.auth_failed(&account, Some(&credential))?;
            return Ok(None);
        }
        Ok(Some(credential))
    }

    pub(in crate::codex_router::proxy) fn observe_activation_route(
        &self,
        attempt: &str,
        thread: &ThreadId,
        observed_account: &AccountId,
    ) -> Result<()> {
        self.activation
            .observe_route(attempt, thread, observed_account, UnixMillis::now().get())
    }

    fn reserve_activation_account(&self, account: &AccountId, thread: &ThreadId) -> Result<bool> {
        let discovered = self.credentials.account_ids();
        self.settings.update(|settings| {
            settings.reconcile(&discovered);
            let reserved = self.runtime.update(|runtime| {
                let at = UnixMillis::now();
                let eligible = settings.enabled()
                    && discovered.contains(account)
                    && !settings.excluded().contains(account)
                    && runtime.is_available(account, at);
                if !eligible {
                    return StoreUpdate::Unchanged(false);
                }
                let reserved = runtime.reserve_thread(account, thread, at).is_ok();
                StoreUpdate::from_changed(reserved, reserved)
            });
            StoreUpdate::Unchanged(reserved)
        })?
    }
}
