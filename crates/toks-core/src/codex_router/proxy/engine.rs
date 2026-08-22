use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use crate::accounts::AccountId;
use crate::rotation::{
    RotationEventKind, RotationRuntime, RotationRuntimeStore, RotationSettingsStore, ThreadId,
    UnixMillis, WaitingThread,
};

use super::types::{CredentialFailure, RouteCredential, SharedCredentials};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

pub(super) struct Engine {
    credentials: SharedCredentials,
    settings: RotationSettingsStore,
    runtime_store: RotationRuntimeStore,
    runtime: Mutex<RotationRuntime>,
}

impl Engine {
    pub fn discover(credentials: SharedCredentials) -> Result<Arc<Self>> {
        let settings = RotationSettingsStore::discover()?;
        let runtime_store = RotationRuntimeStore::discover()?;
        Self::with_stores(credentials, settings, runtime_store)
    }

    pub fn with_stores(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
    ) -> Result<Arc<Self>> {
        let mut runtime = runtime_store.load()?;
        let now = now();
        runtime.reconcile(&credentials.account_ids(), now);
        runtime.heartbeat(now);
        runtime_store.save(&runtime)?;
        Ok(Arc::new(Self {
            credentials,
            settings,
            runtime_store,
            runtime: Mutex::new(runtime),
        }))
    }

    pub async fn select(&self, skipped: &BTreeSet<AccountId>) -> Result<Option<RouteCredential>> {
        loop {
            let Some(account) = self.eligible_account_except(skipped)? else {
                return Ok(None);
            };
            match self.credentials.credential(&account).await {
                Ok(credential) => return Ok(Some(credential)),
                Err(CredentialFailure::NeedsSignIn) => self.auth_failed(&account)?,
                Err(CredentialFailure::Temporary(error)) => return Err(error),
            }
        }
    }

    pub async fn refresh(&self, account: &AccountId) -> Result<Option<RouteCredential>> {
        match self.credentials.refresh(account).await {
            Ok(credential) => Ok(Some(credential)),
            Err(CredentialFailure::NeedsSignIn) => {
                self.auth_failed(account)?;
                Ok(None)
            }
            Err(CredentialFailure::Temporary(error)) => Err(error),
        }
    }

    pub fn eligible_account(&self) -> Result<Option<AccountId>> {
        self.eligible_account_except(&BTreeSet::new())
    }

    fn eligible_account_except(&self, skipped: &BTreeSet<AccountId>) -> Result<Option<AccountId>> {
        let mut settings = self.settings.load()?;
        let discovered = self.credentials.account_ids();
        // The UI owns persisted settings. Reconcile only this in-memory view so
        // a newly enrolled account can route before the next UI poll.
        settings.reconcile(&discovered);
        let runtime = self.runtime.lock().expect("router runtime poisoned");
        if !settings.enabled() {
            return Ok(None);
        }
        Ok(settings
            .preferred()
            .into_iter()
            .chain(settings.priority())
            .find(|account| {
                discovered.contains(account)
                    && !settings.excluded().contains(account)
                    && !skipped.contains(account)
                    && runtime.is_available(account, now())
            })
            .cloned())
    }

    pub fn block(&self, account: &AccountId, reset: Option<UnixMillis>) -> Result<()> {
        let at = now();
        let until = reset
            .or_else(|| earliest_known_reset(account, at))
            .unwrap_or_else(|| UnixMillis::new(at.get() + REPROBE_AFTER_MILLIS));
        self.mutate(|runtime| runtime.block(account, until, at))
    }

    pub fn waiting(&self, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| runtime.waiting(thread, now()))
    }

    pub fn route(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        let at = now();
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
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
        self.runtime_store.save(&runtime)
    }

    pub fn close(&self, account: &AccountId) -> Result<()> {
        self.mutate(|runtime| runtime.connection_closed(account))
    }

    pub fn heartbeat(&self) -> Result<()> {
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        let at = now();
        runtime.reconcile(&self.credentials.account_ids(), at);
        runtime.heartbeat(at);
        self.runtime_store.save(&runtime)
    }

    pub fn waiting_threads(&self) -> Vec<WaitingThread> {
        self.runtime
            .lock()
            .expect("router runtime poisoned")
            .waiting_threads()
            .to_vec()
    }

    pub fn claim_waiting(&self, thread: &ThreadId, account: &AccountId) -> Result<bool> {
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        let claimed = runtime.resumed(thread, account, now());
        if claimed {
            self.runtime_store.save(&runtime)?;
        }
        Ok(claimed)
    }

    fn auth_failed(&self, account: &AccountId) -> Result<()> {
        self.mutate(|runtime| runtime.auth_failed(account, now()))
    }

    pub fn permanent_auth_failure(&self, account: &AccountId) -> Result<()> {
        self.auth_failed(account)
    }

    fn mutate(&self, change: impl FnOnce(&mut RotationRuntime) -> bool) -> Result<()> {
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        if change(&mut runtime) {
            self.runtime_store
                .save(&runtime)
                .context("saving router runtime")?;
        }
        Ok(())
    }
}

fn earliest_known_reset(account: &AccountId, at: UnixMillis) -> Option<UnixMillis> {
    crate::limits::hydrate_all()
        .into_iter()
        .filter(|snapshot| snapshot.account.id == *account)
        .flat_map(|snapshot| snapshot.windows)
        .filter_map(|window| window.resets_at)
        .map(|reset| UnixMillis::new(reset.timestamp_millis()))
        .filter(|reset| *reset > at)
        .min()
}

pub(super) fn now() -> UnixMillis {
    UnixMillis::new(chrono::Utc::now().timestamp_millis())
}
