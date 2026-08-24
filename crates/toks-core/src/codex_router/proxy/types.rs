use std::sync::Arc;

use crate::accounts::AccountId;
use futures_util::future::BoxFuture;

#[derive(Debug, Clone)]
pub(super) struct RouteCredential {
    pub account_id: AccountId,
    pub access_token: String,
    pub chatgpt_account_id: String,
}

impl RouteCredential {
    pub(super) fn fingerprint(&self) -> String {
        crate::accounts::credential_fingerprint(&self.access_token, &self.chatgpt_account_id)
    }
}

#[derive(Debug)]
pub(super) enum CredentialFailure {
    NeedsSignIn,
    Temporary(anyhow::Error),
}

pub(super) trait CredentialSource: Send + Sync {
    fn account_ids(&self) -> Vec<AccountId>;
    fn incoming_account(&self, token: &str) -> Option<AccountId>;
    fn credential<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>>;
    fn refresh<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>>;
}

pub(super) type SharedCredentials = Arc<dyn CredentialSource>;

pub(super) struct LocalCredentials;

impl CredentialSource for LocalCredentials {
    fn account_ids(&self) -> Vec<AccountId> {
        crate::codex_router::credentials::account_ids()
    }

    fn incoming_account(&self, token: &str) -> Option<AccountId> {
        crate::codex_router::credentials::incoming_token_account(token)
    }

    fn credential<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move {
            crate::codex_router::credentials::for_account(account)
                .await
                .map(RouteCredential::from)
                .map_err(CredentialFailure::from)
        })
    }

    fn refresh<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move {
            crate::codex_router::credentials::refresh_account(account)
                .await
                .map(RouteCredential::from)
                .map_err(CredentialFailure::from)
        })
    }
}

impl From<crate::codex_router::credentials::Credential> for RouteCredential {
    fn from(value: crate::codex_router::credentials::Credential) -> Self {
        Self {
            account_id: value.account_id,
            access_token: value.access_token,
            chatgpt_account_id: value.chatgpt_account_id,
        }
    }
}

impl From<crate::codex_router::credentials::CredentialError> for CredentialFailure {
    fn from(value: crate::codex_router::credentials::CredentialError) -> Self {
        match value {
            crate::codex_router::credentials::CredentialError::NeedsSignIn(reason) => {
                drop(reason);
                Self::NeedsSignIn
            }
            crate::codex_router::credentials::CredentialError::Temporary(error) => {
                Self::Temporary(error)
            }
        }
    }
}
