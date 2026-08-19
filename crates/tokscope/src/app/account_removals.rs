use std::collections::{HashMap, HashSet};

use gpui::{AppContext, Context};
use tokscope_core::accounts::{AccountOrderKey, ProviderAccount};
use tokscope_core::LimitSnapshot;

use crate::TokscopeApp;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemovalStatus {
    Ready,
    Confirming,
    Pending,
    Failed(String),
}

#[derive(Default)]
pub(crate) struct AccountRemovals {
    states: HashMap<AccountOrderKey, RemovalStatus>,
    removed: HashSet<AccountOrderKey>,
}

impl AccountRemovals {
    pub(crate) fn status(&self, key: &AccountOrderKey) -> RemovalStatus {
        self.states
            .get(key)
            .cloned()
            .unwrap_or(RemovalStatus::Ready)
    }

    pub(crate) fn filter_refresh(&self, snapshots: &mut Vec<LimitSnapshot>) {
        snapshots.retain(|snapshot| {
            !self
                .removed
                .contains(&AccountOrderKey::from_snapshot(snapshot))
        });
    }

    pub(crate) fn allow(&mut self, key: &AccountOrderKey) {
        self.removed.remove(key);
        self.states.remove(key);
    }

    pub(crate) fn confirm(&mut self, key: AccountOrderKey) {
        if !matches!(self.states.get(&key), Some(RemovalStatus::Pending)) {
            self.states.insert(key, RemovalStatus::Confirming);
        }
    }

    pub(crate) fn cancel_confirmation(&mut self, key: &AccountOrderKey) {
        if matches!(self.states.get(key), Some(RemovalStatus::Confirming)) {
            self.states.remove(key);
        }
    }

    fn begin(&mut self, key: AccountOrderKey) -> bool {
        if matches!(self.states.get(&key), Some(RemovalStatus::Pending)) {
            return false;
        }
        self.states.insert(key, RemovalStatus::Pending);
        true
    }

    fn complete(&mut self, key: AccountOrderKey) {
        self.states.remove(&key);
        self.removed.insert(key);
    }

    fn fail(&mut self, key: AccountOrderKey, error: String) {
        self.states.insert(key, RemovalStatus::Failed(error));
    }
}

pub(crate) fn request_removal(
    app: &mut TokscopeApp,
    key: AccountOrderKey,
    cx: &mut Context<TokscopeApp>,
) {
    let Some(account) = account_for_key(&app.limits, &key) else {
        app.account_removals
            .fail(key, "Account is no longer available.".into());
        cx.notify();
        return;
    };
    if !app.account_removals.begin(key.clone()) {
        return;
    }
    cx.notify();

    cx.spawn(async move |this, cx| {
        let provider = key.provider;
        let result = cx
            .background_spawn(async move {
                tokscope_core::accounts::remove_from_tokscope(provider, &account)
            })
            .await;
        let _ = this.update(cx, |app, cx| {
            match result {
                Ok(_) => {
                    app.account_removals.complete(key.clone());
                    app.limits
                        .retain(|snapshot| AccountOrderKey::from_snapshot(snapshot) != key);
                }
                Err(error) => app
                    .account_removals
                    .fail(key.clone(), format!("Couldn't remove account: {error}")),
            }
            cx.notify();
        });
    })
    .detach();
}

fn account_for_key(snapshots: &[LimitSnapshot], key: &AccountOrderKey) -> Option<ProviderAccount> {
    snapshots
        .iter()
        .find(|snapshot| AccountOrderKey::from_snapshot(snapshot) == *key)
        .map(|snapshot| snapshot.account.clone())
}

#[cfg(test)]
#[path = "account_removals/tests.rs"]
mod tests;
