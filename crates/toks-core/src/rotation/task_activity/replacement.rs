use std::collections::BTreeMap;

use super::{ActiveTask, TaskActivity, TaskActivityConflict, WorkerTaskSnapshot};
use crate::rotation::{ThreadId, UnixMillis, WorkerConnectionOwner};

impl TaskActivity {
    pub(crate) fn replace_worker(
        &mut self,
        owner: WorkerConnectionOwner,
        revision: u64,
        tasks: BTreeMap<ThreadId, ActiveTask>,
    ) -> Result<bool, TaskActivityConflict> {
        self.replace_worker_at(owner, revision, tasks, UnixMillis::now())
    }

    pub(crate) fn replace_worker_at(
        &mut self,
        owner: WorkerConnectionOwner,
        revision: u64,
        tasks: BTreeMap<ThreadId, ActiveTask>,
        observed_at: UnixMillis,
    ) -> Result<bool, TaskActivityConflict> {
        let generation = owner.generation();
        let instance = owner.instance_id();
        let snapshots = self.workers.entry(generation).or_default();
        let Some(current) = snapshots.get_mut(&instance) else {
            snapshots.insert(
                instance,
                WorkerTaskSnapshot {
                    revision,
                    observed_at,
                    tasks,
                },
            );
            return Ok(true);
        };
        if revision < current.revision {
            return Ok(false);
        }
        if revision == current.revision {
            if tasks != current.tasks {
                return Err(TaskActivityConflict::RevisionReused {
                    generation,
                    revision,
                });
            }
            if observed_at <= current.observed_at {
                return Ok(false);
            }
            current.observed_at = observed_at;
            return Ok(true);
        }
        *current = WorkerTaskSnapshot {
            revision,
            observed_at,
            tasks,
        };
        Ok(true)
    }

    pub(crate) fn reconcile_expected_workers(
        &mut self,
        expected: &BTreeMap<u64, u64>,
    ) -> Result<bool, TaskActivityConflict> {
        if let Some((&generation, _)) = expected
            .iter()
            .find(|(generation, instance)| **generation == 0 || **instance == 0)
        {
            return Err(TaskActivityConflict::InvalidWorker { generation });
        }
        let before = self.clone();
        self.expected_workers = Some(expected.clone());
        self.workers.retain(|generation, instances| {
            let Some(expected_instance) = expected.get(generation) else {
                return false;
            };
            instances.retain(|instance, _| instance == expected_instance);
            !instances.is_empty()
        });
        Ok(*self != before)
    }

    pub(super) fn validate(&self) -> Result<(), TaskActivityConflict> {
        let invalid_expected = self.expected_workers.as_ref().and_then(|expected| {
            expected
                .iter()
                .find(|(generation, instance)| **generation == 0 || **instance == 0)
                .map(|(generation, _)| *generation)
        });
        let invalid_snapshot = self.workers.iter().find_map(|(generation, instances)| {
            (*generation == 0 || instances.keys().any(|instance| *instance == 0))
                .then_some(*generation)
        });
        match invalid_expected.or(invalid_snapshot) {
            Some(generation) => Err(TaskActivityConflict::InvalidWorker { generation }),
            None => Ok(()),
        }
    }

    pub(super) fn version(&self) -> u8 {
        self.version
    }
}
