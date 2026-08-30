use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, UnixMillis};

use super::{
    ActiveTask, ActiveTaskRow, GenerationTaskActivity, TaskActivity, TaskActivityUnavailable,
    WorkerTaskSnapshot, TASK_ACTIVITY_FRESHNESS_MILLIS,
};

impl TaskActivity {
    pub fn active_task_rows(&self) -> Result<Vec<ActiveTaskRow>, TaskActivityUnavailable> {
        self.active_task_rows_at(UnixMillis::now())
    }

    pub fn active_task_rows_at(
        &self,
        now: UnixMillis,
    ) -> Result<Vec<ActiveTaskRow>, TaskActivityUnavailable> {
        let mut tasks = BTreeMap::<ThreadId, &ActiveTask>::new();
        for (_, worker) in self.covered_workers_at(now)? {
            for (thread_id, task) in &worker.tasks {
                if let Some(current) = tasks.get(thread_id) {
                    if **current != *task {
                        return Err(TaskActivityUnavailable::ConflictingTask {
                            thread_id: thread_id.clone(),
                        });
                    }
                } else {
                    tasks.insert(thread_id.clone(), task);
                }
            }
        }
        let mut rows = tasks
            .into_iter()
            .map(|(thread_id, task)| ActiveTaskRow {
                thread_id,
                account_id: task.account_id.clone(),
                request_settings: task.request_settings.clone(),
                started_at: task.started_at,
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .started_at
                .cmp(&left.started_at)
                .then_with(|| left.thread_id.cmp(&right.thread_id))
        });
        Ok(rows)
    }

    pub fn active_task_count(&self, account: &AccountId) -> Result<u32, TaskActivityUnavailable> {
        self.active_task_count_at(account, UnixMillis::now())
    }

    pub fn active_task_count_at(
        &self,
        account: &AccountId,
        now: UnixMillis,
    ) -> Result<u32, TaskActivityUnavailable> {
        Ok(self
            .active_task_rows_at(now)?
            .iter()
            .filter(|task| &task.account_id == account)
            .count()
            .try_into()
            .unwrap_or(u32::MAX))
    }

    pub fn generation_activity(
        &self,
    ) -> Result<BTreeMap<u64, GenerationTaskActivity>, TaskActivityUnavailable> {
        self.generation_activity_at(UnixMillis::now())
    }

    pub fn generation_activity_at(
        &self,
        now: UnixMillis,
    ) -> Result<BTreeMap<u64, GenerationTaskActivity>, TaskActivityUnavailable> {
        self.active_task_rows_at(now)?;
        Ok(self
            .covered_workers_at(now)?
            .into_iter()
            .map(|(generation, worker)| {
                let oldest_task_at = worker.tasks.values().map(|task| task.started_at).min();
                let task_count = worker.tasks.len().try_into().unwrap_or(u32::MAX);
                (
                    generation,
                    GenerationTaskActivity {
                        task_count,
                        oldest_task_at,
                    },
                )
            })
            .collect())
    }

    fn covered_workers_at(
        &self,
        now: UnixMillis,
    ) -> Result<Vec<(u64, &WorkerTaskSnapshot)>, TaskActivityUnavailable> {
        let expected = self
            .expected_workers
            .as_ref()
            .ok_or(TaskActivityUnavailable::TopologyUnknown)?;
        expected
            .iter()
            .map(|(generation, instance)| {
                let worker = self
                    .workers
                    .get(generation)
                    .and_then(|instances| instances.get(instance))
                    .ok_or(TaskActivityUnavailable::MissingWorker {
                        generation: *generation,
                    })?;
                validate_freshness(*generation, worker.observed_at, now)?;
                Ok((*generation, worker))
            })
            .collect()
    }
}

fn validate_freshness(
    generation: u64,
    observed_at: UnixMillis,
    now: UnixMillis,
) -> Result<(), TaskActivityUnavailable> {
    let age = now.get().saturating_sub(observed_at.get());
    if age < -TASK_ACTIVITY_FRESHNESS_MILLIS {
        return Err(TaskActivityUnavailable::FutureWorker { generation });
    }
    if age > TASK_ACTIVITY_FRESHNESS_MILLIS {
        return Err(TaskActivityUnavailable::StaleWorker { generation });
    }
    Ok(())
}
