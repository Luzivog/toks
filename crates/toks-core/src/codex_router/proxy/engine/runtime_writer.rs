use std::sync::Mutex;

use anyhow::Result;

use crate::rotation::{RotationRuntime, RotationRuntimeStore};
use crate::storage::StoreUpdate;

pub(super) struct RuntimeWriter {
    store: RotationRuntimeStore,
    cached: Mutex<RotationRuntime>,
}

impl RuntimeWriter {
    pub(super) fn new(store: RotationRuntimeStore) -> Result<Self> {
        Ok(Self {
            store,
            cached: Mutex::new(RotationRuntime::default()),
        })
    }

    pub(super) fn update<T>(
        &self,
        change: impl FnOnce(&mut RotationRuntime) -> StoreUpdate<T>,
    ) -> Result<T> {
        let mut cached = self.cached.lock().expect("router runtime poisoned");
        let (value, runtime) = self
            .store
            .update(|runtime| change(runtime).map(|value| (value, runtime.clone())))?;
        *cached = runtime;
        Ok(value)
    }

    pub(super) fn latest<T>(&self, inspect: impl FnOnce(&RotationRuntime) -> T) -> Result<T> {
        self.update(|runtime| StoreUpdate::Unchanged(inspect(runtime)))
    }

    pub(super) fn cached<T>(&self, inspect: impl FnOnce(&RotationRuntime) -> T) -> T {
        inspect(&self.cached.lock().expect("router runtime poisoned"))
    }
}
