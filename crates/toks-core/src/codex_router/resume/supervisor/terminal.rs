use anyhow::Result;

use super::{
    ResumeAttempt, ResumePhase, ResumeQueue, Supervisor, TaskState, TaskUnits, ThreadId,
    UnixMillis, RETRY_DELAY_MILLIS,
};
use crate::rotation::ResumeTerminal;

impl<Q: ResumeQueue, U: TaskUnits> Supervisor<Q, U> {
    pub(super) fn mark_running(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
    ) -> Result<()> {
        if state.attempts[thread].phase != ResumePhase::Running {
            state.attempts.get_mut(thread).unwrap().phase = ResumePhase::Running;
            self.store.save(state)?;
        }
        Ok(())
    }

    pub(super) fn stage_finished(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
        attempt: &ResumeAttempt,
        success: bool,
        now: UnixMillis,
    ) -> Result<()> {
        let terminal = if success {
            super::super::state::ResumeTerminalState::Success
        } else {
            super::super::state::ResumeTerminalState::Failure
        };
        self.stage_terminal(state, thread, attempt, terminal, now)
    }

    pub(super) fn stage_cancelled(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
        attempt: &ResumeAttempt,
        task: TaskState,
        now: UnixMillis,
    ) -> Result<()> {
        self.units.cancel(&attempt.id, task)?;
        self.stage_terminal(
            state,
            thread,
            attempt,
            super::super::state::ResumeTerminalState::Cancelled,
            now,
        )
    }

    pub(super) fn stage_discarded(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
        attempt: &ResumeAttempt,
        task: TaskState,
        now: UnixMillis,
    ) -> Result<()> {
        if attempt.phase != ResumePhase::Authorizing {
            self.units.cancel(&attempt.id, task)?;
        }
        self.stage_terminal(
            state,
            thread,
            attempt,
            super::super::state::ResumeTerminalState::Discarded,
            now,
        )
    }

    pub(super) fn stage_abandoned(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
    ) -> Result<()> {
        let current = state.attempts.get_mut(thread).expect("current attempt");
        current.phase = ResumePhase::Cleaning;
        current.terminal = Some(super::super::state::ResumeTerminalState::Abandoned);
        self.store.save(state)?;
        self.cleanup_terminal(state, thread)
    }

    pub(super) fn stage_unlaunched_cancel(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
        attempt: &ResumeAttempt,
        _now: UnixMillis,
    ) -> Result<()> {
        let _ = self.queue.finish(
            &attempt.waiting,
            &attempt.id,
            ResumeTerminal::Cancelled,
            attempt.retry_waiting_id.clone(),
        )?;
        let current = state.attempts.get_mut(thread).expect("current attempt");
        current.phase = ResumePhase::Cleaning;
        current.terminal = Some(super::super::state::ResumeTerminalState::Cancelled);
        self.store.save(state)?;
        self.cleanup_terminal(state, thread)
    }

    fn stage_terminal(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
        attempt: &ResumeAttempt,
        terminal: super::super::state::ResumeTerminalState,
        now: UnixMillis,
    ) -> Result<()> {
        let queue_terminal = match terminal {
            super::super::state::ResumeTerminalState::Success => ResumeTerminal::Success,
            super::super::state::ResumeTerminalState::Failure => ResumeTerminal::Failure,
            super::super::state::ResumeTerminalState::Cancelled => ResumeTerminal::Cancelled,
            super::super::state::ResumeTerminalState::Discarded => ResumeTerminal::Discarded,
            super::super::state::ResumeTerminalState::Abandoned => unreachable!(),
        };
        let queued = self.queue.finish(
            &attempt.waiting,
            &attempt.id,
            queue_terminal,
            attempt.retry_waiting_id.clone(),
        )?;
        state.retry_after.remove(&attempt.waiting.waiting_id);
        if terminal == super::super::state::ResumeTerminalState::Failure {
            if let Some(waiting) = queued {
                state.retry_after.insert(
                    waiting.waiting_id,
                    UnixMillis::new(now.get() + RETRY_DELAY_MILLIS),
                );
            }
        }
        let current = state.attempts.get_mut(thread).expect("current attempt");
        current.phase = ResumePhase::Cleaning;
        current.terminal = Some(terminal);
        self.store.save(state)?;
        self.cleanup_terminal(state, thread)
    }

    pub(super) fn reconcile_cleaning(&mut self, state: &mut super::ResumeState) -> Result<()> {
        let cleaning = state
            .attempts
            .iter()
            .filter(|(_, attempt)| attempt.phase == ResumePhase::Cleaning)
            .map(|(thread, _)| thread.clone())
            .collect::<Vec<_>>();
        let mut errors = Vec::new();
        for thread in cleaning {
            super::errors::record(
                &mut errors,
                &format!("cleaning resume attempt for {}", thread.as_str()),
                self.cleanup_terminal(state, &thread),
            );
        }
        super::errors::finish(errors)
    }

    fn cleanup_terminal(
        &mut self,
        state: &mut super::ResumeState,
        thread: &ThreadId,
    ) -> Result<()> {
        let attempt = state.attempts[thread].clone();
        let abandoned =
            attempt.terminal == Some(super::super::state::ResumeTerminalState::Abandoned);
        if !abandoned {
            self.units.cleanup(&attempt.id)?;
        }
        self.store.remove_outcome(&attempt.id)?;
        if !abandoned {
            self.queue.forget(&attempt.waiting, &attempt.id)?;
        }
        state.attempts.remove(thread);
        self.store.save(state)
    }
}
