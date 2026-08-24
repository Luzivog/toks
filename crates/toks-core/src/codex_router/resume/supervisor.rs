use anyhow::Result;
use std::collections::BTreeMap;

use super::state::{ResumeAttempt, ResumePhase, ResumeState, ResumeStore};
use crate::rotation::{RotationSettingsStore, ThreadId, UnixMillis};

mod errors;
mod launch;
mod prune;
mod reconcile;
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
}
