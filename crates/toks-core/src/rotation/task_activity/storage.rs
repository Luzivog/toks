use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use super::{ActiveTask, TaskActivity, TASK_ACTIVITY_VERSION};
#[cfg(test)]
use crate::rotation::UnixMillis;
use crate::rotation::{ThreadId, WorkerConnectionOwner};
use crate::storage::StoreUpdate;

const LABEL: &str = "task activity";

#[derive(Debug, Clone)]
pub struct TaskActivityStore {
    path: PathBuf,
}

impl TaskActivityStore {
    pub fn discover() -> Result<Self> {
        Ok(Self::at(crate::paths::rotation_task_activity()?))
    }

    pub fn for_data_dir(root: impl AsRef<Path>) -> Self {
        Self::at(crate::paths::rotation_task_activity_at(root.as_ref()))
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<TaskActivity> {
        let Some(activity): Option<TaskActivity> =
            super::super::storage::read_json(&self.path, LABEL)?
        else {
            return Ok(TaskActivity::default());
        };
        if activity.version() != TASK_ACTIVITY_VERSION {
            bail!("unsupported task activity version {}", activity.version());
        }
        activity.validate()?;
        Ok(activity)
    }

    pub(crate) fn replace_worker(
        &self,
        owner: WorkerConnectionOwner,
        revision: u64,
        tasks: BTreeMap<ThreadId, ActiveTask>,
    ) -> Result<bool> {
        self.update(
            |activity| match activity.replace_worker(owner, revision, tasks) {
                Ok(changed) => StoreUpdate::from_changed(Ok(changed), changed),
                Err(error) => StoreUpdate::Unchanged(Err(error)),
            },
        )?
        .map_err(Into::into)
    }

    #[cfg(test)]
    pub(crate) fn replace_worker_at(
        &self,
        owner: WorkerConnectionOwner,
        revision: u64,
        tasks: BTreeMap<ThreadId, ActiveTask>,
        observed_at: UnixMillis,
    ) -> Result<bool> {
        self.update(|activity| {
            match activity.replace_worker_at(owner, revision, tasks, observed_at) {
                Ok(changed) => StoreUpdate::from_changed(Ok(changed), changed),
                Err(error) => StoreUpdate::Unchanged(Err(error)),
            }
        })?
        .map_err(Into::into)
    }

    pub(crate) fn reconcile_expected_workers(&self, expected: &BTreeMap<u64, u64>) -> Result<bool> {
        self.update(
            |activity| match activity.reconcile_expected_workers(expected) {
                Ok(changed) => StoreUpdate::from_changed(Ok(changed), changed),
                Err(error) => StoreUpdate::Unchanged(Err(error)),
            },
        )?
        .map_err(Into::into)
    }

    fn update<T>(&self, change: impl FnOnce(&mut TaskActivity) -> StoreUpdate<T>) -> Result<T> {
        let _lock = super::super::storage::lock_document(&self.path, LABEL)?;
        let mut activity = self.load()?;
        let (value, changed) = change(&mut activity).into_parts();
        if changed {
            activity.validate()?;
            super::super::storage::write_json(&self.path, &activity, LABEL)?;
        }
        Ok(value)
    }
}
