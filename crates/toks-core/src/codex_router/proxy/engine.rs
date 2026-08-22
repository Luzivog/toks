use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};

use crate::accounts::AccountId;
use crate::rotation::{
    RotationEventKind, RotationRuntime, RotationRuntimeStore, RotationSettingsStore, ThreadId,
    UnixMillis, WaitingThread,
};

use super::catalogue::Catalogue;
use super::types::{CredentialFailure, RouteCredential, SharedCredentials};

mod quota;
mod selection;

pub(super) struct Engine {
    credentials: SharedCredentials,
    settings: RotationSettingsStore,
    runtime_store: RotationRuntimeStore,
    runtime: Mutex<RotationRuntime>,
    catalogue: Catalogue,
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
        Self::with_catalogue(credentials, settings, runtime_store, Catalogue::discover())
    }

    pub fn with_catalogue(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
        catalogue: Catalogue,
    ) -> Result<Arc<Self>> {
        let mut runtime = runtime_store.load()?;
        let now = now();
        runtime.reconcile(&credentials.account_ids(), now);
        runtime.reset_connections();
        runtime.heartbeat(now);
        runtime_store.save(&runtime)?;
        Ok(Arc::new(Self {
            credentials,
            settings,
            runtime_store,
            runtime: Mutex::new(runtime),
            catalogue,
        }))
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

    pub fn attach(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| runtime.thread_attached(account, thread))
    }

    pub fn detach(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| runtime.thread_detached(account, thread))
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

pub(super) fn now() -> UnixMillis {
    UnixMillis::new(chrono::Utc::now().timestamp_millis())
}
