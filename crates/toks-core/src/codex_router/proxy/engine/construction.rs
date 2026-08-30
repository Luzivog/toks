use std::sync::Arc;
use std::sync::Mutex;

use anyhow::Result;

use crate::codex_router::thread_source::ThreadSourceStore;
use crate::rotation::{
    RotationRuntimeStore, RotationSettingsStore, TaskActivityStore, UnixMillis,
    WorkerConnectionOwner,
};
use crate::storage::StoreUpdate;

use super::{Catalogue, Engine, RuntimeWriter, SharedCredentials, TaskActivityPublisher};

pub(in crate::codex_router::proxy) struct EngineConfig {
    pub(in crate::codex_router::proxy) credentials: SharedCredentials,
    pub(in crate::codex_router::proxy) settings: RotationSettingsStore,
    pub(in crate::codex_router::proxy) runtime_store: RotationRuntimeStore,
    pub(in crate::codex_router::proxy) catalogue: Catalogue,
    pub(in crate::codex_router::proxy) connection_owner: Option<WorkerConnectionOwner>,
    pub(in crate::codex_router::proxy) thread_sources: ThreadSourceStore,
    pub(in crate::codex_router::proxy) task_activity_store: Option<TaskActivityStore>,
}

impl EngineConfig {
    pub(in crate::codex_router::proxy) fn discover(credentials: SharedCredentials) -> Result<Self> {
        let task_activity_store = match TaskActivityStore::discover() {
            Ok(store) => Some(store),
            Err(error) => {
                eprintln!("router could not discover task activity storage: {error:#}");
                None
            }
        };
        Ok(Self {
            credentials,
            settings: RotationSettingsStore::discover()?,
            runtime_store: RotationRuntimeStore::discover()?,
            catalogue: Catalogue::discover(),
            connection_owner: None,
            thread_sources: ThreadSourceStore::discover(),
            task_activity_store,
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
            task_activity_store,
        } = config;
        let activation =
            crate::codex_router::account_activation::Store::for_runtime(&runtime_store);
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
        let task_activity = TaskActivityPublisher::new(connection_owner, task_activity_store);
        let engine = Arc::new(Self {
            credentials,
            settings,
            runtime,
            catalogue,
            connection_owner,
            connection_inventory: Mutex::new(Default::default()),
            task_activity,
            thread_sources,
            activation,
        });
        engine.apply_rotation_settings(observed_at)?;
        Ok(engine)
    }
}
