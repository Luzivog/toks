use std::sync::Mutex;

use anyhow::Result;

use crate::rotation::{RotationRuntime, RotationRuntimeStore};

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
        change: impl FnOnce(&mut RotationRuntime) -> (T, bool),
    ) -> Result<T> {
        let mut cached = self.cached.lock().expect("router runtime poisoned");
        let (value, runtime) = self.store.update(|runtime| {
            let (value, changed) = change(runtime);
            ((value, runtime.clone()), changed)
        })?;
        *cached = runtime;
        Ok(value)
    }

    pub(super) fn latest<T>(&self, inspect: impl FnOnce(&RotationRuntime) -> T) -> Result<T> {
        self.update(|runtime| (inspect(runtime), false))
    }

    pub(super) fn cached<T>(&self, inspect: impl FnOnce(&RotationRuntime) -> T) -> T {
        inspect(&self.cached.lock().expect("router runtime poisoned"))
    }
}
