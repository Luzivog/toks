use anyhow::Result;
use std::collections::BTreeMap;

use super::state::{ResumeAttempt, ResumePhase, ResumeState, ResumeStore};
use crate::rotation::{RotationSettings, RotationSettingsStore, ThreadId, UnixMillis};

mod errors;
mod launch;
mod prune;
#[cfg(test)]
pub(in crate::codex_router::resume) use launch::AuthorizationOutcome;
mod queue;
mod terminal;
pub(in crate::codex_router::resume) use queue::ResumeQueue;

const RETRY_DELAY_MILLIS: i64 = 5 * 60 * 1_000;
type WorkspaceLookup = Box<dyn Fn(&ThreadId) -> Result<std::path::PathBuf>>;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum TaskState {
    Missing,
    Starting,
    Running,
    Succeeded,
    Failed,
}

pub(super) trait TaskUnits {
    fn launch(&mut self, attempt: &ResumeAttempt) -> Result<()>;
    fn inventory(&mut self, attempts: &[String]) -> Result<BTreeMap<String, TaskState>>;
    fn cleanup(&mut self, attempt: &str) -> Result<()>;
    fn cancel(&mut self, attempt: &str, state: TaskState) -> Result<()>;
}

pub(super) struct Supervisor<Q, U> {
    store: ResumeStore,
    settings: RotationSettingsStore,
    queue: Q,
    units: U,
    workspace: WorkspaceLookup,
    thread_sources: crate::codex_router::thread_source::ThreadSourceStore,
}

impl<Q: ResumeQueue, U: TaskUnits> Supervisor<Q, U> {
    pub(super) fn new(store: ResumeStore, queue: Q, units: U) -> Result<Self> {
        Ok(Self {
            store,
            settings: RotationSettingsStore::discover()?,
            queue,
            units,
            workspace: Box::new(super::workspace::discover),
            thread_sources: crate::codex_router::thread_source::ThreadSourceStore::discover(),
        })
    }

    #[cfg(test)]
    pub(super) fn for_test(
        store: ResumeStore,
        settings: RotationSettingsStore,
        queue: Q,
        units: U,
        workspace: impl Fn(&ThreadId) -> Result<std::path::PathBuf> + 'static,
    ) -> Self {
        Self::for_test_with_thread_sources(
            store,
            settings,
            queue,
            units,
            workspace,
            crate::codex_router::thread_source::ThreadSourceStore::unavailable(),
        )
    }

    #[cfg(test)]
    pub(super) fn for_test_with_thread_sources(
        store: ResumeStore,
        settings: RotationSettingsStore,
        queue: Q,
        units: U,
        workspace: impl Fn(&ThreadId) -> Result<std::path::PathBuf> + 'static,
        thread_sources: crate::codex_router::thread_source::ThreadSourceStore,
    ) -> Self {
        Self {
            store,
            settings,
            queue,
            units,
            workspace: Box::new(workspace),
            thread_sources,
        }
    }

    pub(super) fn tick(&mut self, now: UnixMillis) -> Result<()> {
        let mut state = self.store.load()?;
        let settings = self.settings.load()?;
        let mut errors = Vec::new();
        let pruned = self.prune_known_subagents();
        let may_launch = pruned.is_ok();
        errors::record(&mut errors, "pruning non-resumable subagent tasks", pruned);
        errors::record(
            &mut errors,
            "reconciling terminal attempts",
            self.reconcile_cleaning(&mut state),
        );
        errors::record(
            &mut errors,
            "reconciling active attempts",
            self.reconcile_attempts(&mut state, &settings, now),
        );
        errors::record(
            &mut errors,
            "pruning retry delays",
            self.prune_retries(&mut state),
        );
        if may_launch {
            errors::record(
                &mut errors,
                "launching next waiting task",
                self.launch_next(&mut state, &settings, now),
            );
        }
        errors::finish(errors)
    }

    fn reconcile_attempts(
        &mut self,
        state: &mut ResumeState,
        settings: &RotationSettings,
        now: UnixMillis,
    ) -> Result<()> {
        let threads = state.attempts.keys().cloned().collect::<Vec<_>>();
        let mut outcomes = BTreeMap::new();
        for thread in &threads {
            if let Some(success) = self.store.outcome(&state.attempts[thread].id)? {
                outcomes.insert(thread.clone(), success);
            }
        }
        let attempts = threads
            .iter()
            .filter(|thread| {
                !outcomes.contains_key(*thread)
                    && state.attempts[*thread].phase != ResumePhase::Authorizing
                    && state.attempts[*thread].phase != ResumePhase::Cleaning
            })
            .map(|thread| state.attempts[thread].id.clone())
            .collect::<Vec<_>>();
        let inventory = self.units.inventory(&attempts)?;
        for thread in threads {
            let attempt = state.attempts[&thread].clone();
            if attempt.phase == ResumePhase::Cleaning {
                continue;
            }
            if self.thread_sources.is_known_subagent(&thread) {
                let task = inventory
                    .get(&attempt.id)
                    .copied()
                    .unwrap_or(TaskState::Missing);
                self.stage_discarded(state, &thread, &attempt, task, now)?;
                continue;
            }
            if let Some(success) = outcomes.remove(&thread) {
                self.stage_finished(state, &thread, &attempt, success, now)?;
                continue;
            }
            if attempt.phase == ResumePhase::Authorizing {
                if settings.cancelled_threads().contains(&thread) {
                    self.stage_unlaunched_cancel(state, &thread, &attempt, now)?;
                } else {
                    self.authorize(state, &thread, &attempt, now)?;
                }
                continue;
            }
            let task = inventory
                .get(&attempt.id)
                .copied()
                .unwrap_or(TaskState::Missing);
            match task {
                TaskState::Missing
                | TaskState::Starting
                | TaskState::Running
                | TaskState::Failed
                    if settings.cancelled_threads().contains(&thread) =>
                {
                    self.stage_cancelled(state, &thread, &attempt, task, now)?
                }
                TaskState::Missing if attempt.phase == ResumePhase::Launching => {
                    self.authorize(state, &thread, &attempt, now)?;
                }
                TaskState::Missing => self.stage_finished(state, &thread, &attempt, false, now)?,
                TaskState::Starting => {}
                TaskState::Running => self.mark_running(state, &thread)?,
                TaskState::Succeeded => self.stage_finished(state, &thread, &attempt, true, now)?,
                TaskState::Failed => self.stage_finished(state, &thread, &attempt, false, now)?,
            }
        }
        Ok(())
    }
}
