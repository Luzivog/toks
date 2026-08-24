use std::sync::{Arc, Mutex};

use futures_util::future::BoxFuture;

use crate::accounts::{AccountId, ProviderAccount};
use crate::limits::{LimitSnapshot, Provider};

use super::super::super::types::{CredentialFailure, CredentialSource, RouteCredential};

mod proof_tests;
mod race_tests;

#[derive(Clone, Copy)]
enum CredentialState {
    Valid(&'static str),
    NeedsSignIn,
    Unreadable,
    WrongAccount,
}

struct RepairableCredentials {
    account: AccountId,
    discovered: Mutex<bool>,
    state: Mutex<CredentialState>,
    proof_gate: Mutex<
        Option<(
            tokio::sync::mpsc::UnboundedSender<()>,
            Arc<tokio::sync::Notify>,
        )>,
    >,
}

impl CredentialSource for RepairableCredentials {
    fn account_ids(&self) -> Vec<AccountId> {
        self.discovered
            .lock()
            .unwrap()
            .then(|| self.account.clone())
            .into_iter()
            .collect()
    }

    fn incoming_account(&self, _token: &str) -> Option<AccountId> {
        None
    }

    fn credential<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        Box::pin(async move {
            let gate = self.proof_gate.lock().unwrap().clone();
            if let Some((started, proceed)) = gate {
                let _ = started.send(());
                proceed.notified().await;
            }
            match *self.state.lock().unwrap() {
                CredentialState::Valid(token) => Ok(credential_with_token(account, token)),
                CredentialState::NeedsSignIn => Err(CredentialFailure::NeedsSignIn),
                CredentialState::Unreadable => Err(CredentialFailure::Temporary(anyhow::anyhow!(
                    "synthetic unreadable credential"
                ))),
                CredentialState::WrongAccount => Ok(credential(&AccountId::new("wrong"))),
            }
        })
    }

    fn refresh<'a>(
        &'a self,
        account: &'a AccountId,
    ) -> BoxFuture<'a, Result<RouteCredential, CredentialFailure>> {
        self.credential(account)
    }
}

fn snapshot(account: &AccountId) -> LimitSnapshot {
    LimitSnapshot::loading_account(
        Provider::Codex,
        ProviderAccount {
            id: account.clone(),
            ..ProviderAccount::unidentified_for(Provider::Codex)
        },
    )
}

fn credential(account: &AccountId) -> RouteCredential {
    credential_with_token(account, "repaired-token")
}

fn credential_with_token(account: &AccountId, token: &str) -> RouteCredential {
    RouteCredential {
        account_id: account.clone(),
        access_token: token.into(),
        chatgpt_account_id: "chatgpt-repaired".into(),
    }
}
