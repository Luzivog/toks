use anyhow::Result;

use super::super::super::types::{CredentialFailure, RouteCredential};
use super::super::Engine;
use super::verified_credential;

impl Engine {
    pub async fn refresh(&self, rejected: &RouteCredential) -> Result<Option<RouteCredential>> {
        let account = &rejected.account_id;
        match self.credentials.refresh(account).await {
            Ok(credential) => {
                let credential = verified_credential(account, credential)?;
                if self.credential_was_rejected(&credential)? {
                    self.auth_failed(account, Some(&credential))?;
                    return Ok(None);
                }
                Ok(Some(credential))
            }
            Err(CredentialFailure::NeedsSignIn) => {
                self.auth_failed(account, Some(rejected))?;
                Ok(None)
            }
            Err(CredentialFailure::Temporary(error)) => Err(error),
        }
    }

    pub(super) fn credential_was_rejected(&self, credential: &RouteCredential) -> Result<bool> {
        let fingerprint = credential.fingerprint();
        self.runtime
            .latest(|runtime| runtime.credential_was_rejected(&credential.account_id, &fingerprint))
    }
}
