use std::sync::Arc;

use anyhow::Result;

use crate::rotation::{RotationRuntimeStore, RotationSettingsStore, WorkerConnectionOwner};

use super::{now, Catalogue, Engine, RuntimeWriter, SharedCredentials};
use crate::codex_router::thread_source::ThreadSourceStore;

impl Engine {
    pub fn discover(credentials: SharedCredentials) -> Result<Arc<Self>> {
        Self::discover_with_owner(credentials, None)
    }

    pub fn discover_for_worker(
        credentials: SharedCredentials,
        generation: u64,
        instance_id: u64,
    ) -> Result<Arc<Self>> {
        let owner = WorkerConnectionOwner::new(generation, instance_id)
            .ok_or_else(|| anyhow::anyhow!("router worker identity must be nonzero"))?;
        Self::discover_with_owner(credentials, Some(owner))
    }

    fn discover_with_owner(
        credentials: SharedCredentials,
        connection_owner: Option<WorkerConnectionOwner>,
    ) -> Result<Arc<Self>> {
        let settings = RotationSettingsStore::discover()?;
        let runtime_store = RotationRuntimeStore::discover()?;
        if connection_owner.is_none() {
            return Self::with_stores(credentials, settings, runtime_store);
        }
        Self::with_catalogue_and_owner(
            credentials,
            settings,
            runtime_store,
            Catalogue::discover(),
            connection_owner,
        )
    }

    pub fn with_stores(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
    ) -> Result<Arc<Self>> {
        Self::with_catalogue_and_owner(
            credentials,
            settings,
            runtime_store,
            Catalogue::discover(),
            None,
        )
    }

    #[cfg(test)]
    pub fn with_catalogue(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
        catalogue: Catalogue,
    ) -> Result<Arc<Self>> {
        Self::with_catalogue_and_owner(credentials, settings, runtime_store, catalogue, None)
    }

    #[cfg(test)]
    pub fn with_catalogue_for_worker(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
        catalogue: Catalogue,
        generation: u64,
        instance_id: u64,
    ) -> Result<Arc<Self>> {
        let owner = WorkerConnectionOwner::new(generation, instance_id)
            .ok_or_else(|| anyhow::anyhow!("router worker identity must be nonzero"))?;
        Self::with_catalogue_and_owner(credentials, settings, runtime_store, catalogue, Some(owner))
    }

    fn with_catalogue_and_owner(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
        catalogue: Catalogue,
        connection_owner: Option<WorkerConnectionOwner>,
    ) -> Result<Arc<Self>> {
        Self::with_catalogue_owner_and_sources(
            credentials,
            settings,
            runtime_store,
            catalogue,
            connection_owner,
            ThreadSourceStore::discover(),
        )
    }

    #[cfg(test)]
    pub fn with_catalogue_and_thread_sources(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
        catalogue: Catalogue,
        thread_sources: ThreadSourceStore,
    ) -> Result<Arc<Self>> {
        Self::with_catalogue_owner_and_sources(
            credentials,
            settings,
            runtime_store,
            catalogue,
            None,
            thread_sources,
        )
    }

    fn with_catalogue_owner_and_sources(
        credentials: SharedCredentials,
        settings: RotationSettingsStore,
        runtime_store: RotationRuntimeStore,
        catalogue: Catalogue,
        connection_owner: Option<WorkerConnectionOwner>,
        thread_sources: ThreadSourceStore,
    ) -> Result<Arc<Self>> {
        let runtime = RuntimeWriter::new(runtime_store)?;
        let observed_at = now();
        runtime.update(|state| {
            state.reconcile(&credentials.account_ids(), observed_at);
            if let Some(owner) = connection_owner {
                state.adopt_worker_instance(owner);
            }
            state.heartbeat(observed_at);
            ((), true)
        })?;
        Ok(Arc::new(Self {
            credentials,
            settings,
            runtime,
            catalogue,
            connection_owner,
            thread_sources,
        }))
    }
}
