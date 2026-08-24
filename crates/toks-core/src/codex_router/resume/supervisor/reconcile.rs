use std::collections::BTreeMap;

use anyhow::Result;

use super::{ResumePhase, ResumeQueue, Supervisor, TaskState, TaskUnits};
use crate::codex_router::resume::state::ResumeState;
use crate::rotation::{ResumeTerminal, RotationSettings, UnixMillis};

impl<Q: ResumeQueue, U: TaskUnits> Supervisor<Q, U> {
    pub(super) fn reconcile_attempts(
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
                let terminal = if success {
                    ResumeTerminal::Success
                } else {
                    ResumeTerminal::Failure
                };
                self.stage_terminal(state, &thread, &attempt, terminal, now)?;
                continue;
            }
            if attempt.phase == ResumePhase::Authorizing {
                if settings.cancelled_threads().contains(&thread) {
                    self.stage_unlaunched_cancel(state, &thread, &attempt)?;
                } else {
                    self.authorize(state, &thread, &attempt)?;
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
                    self.authorize(state, &thread, &attempt)?;
                }
                TaskState::Missing => {
                    self.stage_terminal(state, &thread, &attempt, ResumeTerminal::Failure, now)?
                }
                TaskState::Starting => {}
                TaskState::Running => self.mark_running(state, &thread)?,
                TaskState::Succeeded => {
                    self.stage_terminal(state, &thread, &attempt, ResumeTerminal::Success, now)?
                }
                TaskState::Failed => {
                    self.stage_terminal(state, &thread, &attempt, ResumeTerminal::Failure, now)?
                }
            }
        }
        Ok(())
    }
}
