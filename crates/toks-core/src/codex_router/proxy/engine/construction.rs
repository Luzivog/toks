use std::sync::Arc;

use anyhow::Result;

use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{
    RotationRuntimeStore, RotationSettingsStore, UnixMillis, WorkerConnectionOwner,
};
use crate::storage::StoreUpdate;

use super::{Catalogue, Engine, RuntimeWriter, SharedCredentials};

pub(in crate::codex_router::proxy) struct EngineConfig {
    pub(in crate::codex_router::proxy) credentials: SharedCredentials,
    pub(in crate::codex_router::proxy) settings: RotationSettingsStore,
    pub(in crate::codex_router::proxy) runtime_store: RotationRuntimeStore,
    pub(in crate::codex_router::proxy) catalogue: Catalogue,
    pub(in crate::codex_router::proxy) connection_owner: Option<WorkerConnectionOwner>,
    pub(in crate::codex_router::proxy) thread_sources: ThreadSourceStore,
}

impl EngineConfig {
    pub(in crate::codex_router::proxy) fn discover(credentials: SharedCredentials) -> Result<Self> {
        Ok(Self {
            credentials,
            settings: RotationSettingsStore::discover()?,
            runtime_store: RotationRuntimeStore::discover()?,
            catalogue: Catalogue::discover(),
            connection_owner: None,
            thread_sources: ThreadSourceStore::discover(),
        })
    }
}

impl Engine {
    pub(in crate::codex_router::proxy) fn new(config: EngineConfig) -> Result<Arc<Self>> {
        let EngineConfig {
            credentials,
            settings,
            runtime_store,
            catalogue,
            connection_owner,
            thread_sources,
        } = config;
        let runtime = RuntimeWriter::new(runtime_store)?;
        let observed_at = UnixMillis::now();
        runtime.update(|state| {
            state.reconcile(&credentials.account_ids(), observed_at);
            if let Some(owner) = connection_owner {
                state.adopt_worker_instance(owner);
            }
            state.heartbeat(observed_at);
            StoreUpdate::Changed(())
        })?;
        let engine = Arc::new(Self {
            credentials,
            settings,
            runtime,
            catalogue,
            connection_owner,
            thread_sources,
        });
        engine.reconcile_thread_overrides(observed_at)?;
        Ok(engine)
    }
}
