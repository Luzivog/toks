use std::sync::Arc;

use futures_util::future::BoxFuture;

use crate::accounts::AccountId;

#[derive(Debug, Clone)]
pub(super) struct RouteCredential {
    pub account_id: AccountId,
    pub access_token: String,
    pub chatgpt_account_id: String,
}

#[derive(Debug)]
pub(super) enum CredentialFailure {
    NeedsSignIn,
    Temporary(anyhow::Error),
}

pub(super) trait CredentialSource: Send + Sync {
    fn account_ids(&self) -> Vec<AccountId>;
    fn accepts_incoming(&self, token: &str) -> bool;
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
        super::super::credentials::account_ids()
    }

    fn accepts_incoming(&self, token: &str) -> bool {
        super::super::credentials::incoming_token_is_enrolled(token)
    }

    fn credential<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move {
            super::super::credentials::for_account(account)
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
            super::super::credentials::refresh_account(account)
                .await
                .map(RouteCredential::from)
                .map_err(CredentialFailure::from)
        })
    }
}

impl From<super::super::credentials::Credential> for RouteCredential {
    fn from(value: super::super::credentials::Credential) -> Self {
        Self {
            account_id: value.account_id,
            access_token: value.access_token,
            chatgpt_account_id: value.chatgpt_account_id,
        }
    }
}

impl From<super::super::credentials::CredentialError> for CredentialFailure {
    fn from(value: super::super::credentials::CredentialError) -> Self {
        match value {
            super::super::credentials::CredentialError::NeedsSignIn(reason) => {
                drop(reason);
                Self::NeedsSignIn
            }
            super::super::credentials::CredentialError::Temporary(error) => Self::Temporary(error),
        }
    }
}
