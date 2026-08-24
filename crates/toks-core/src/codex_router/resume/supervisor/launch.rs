use anyhow::Result;
use std::collections::BTreeSet;

use super::{ResumeAttempt, ResumePhase, ResumeQueue, Supervisor, TaskUnits, RETRY_DELAY_MILLIS};
use crate::rotation::{RotationSettings, UnixMillis, WaitingId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::codex_router::resume) enum AuthorizationOutcome {
    Launched,
    Retry,
    Cancelled,
}

impl<Q: ResumeQueue, U: TaskUnits> Supervisor<Q, U> {
    pub(super) fn launch_next(
        &mut self,
        state: &mut super::ResumeState,
        settings: &RotationSettings,
        now: UnixMillis,
    ) -> Result<()> {
        let waiting = self.queue.waiting_threads();
        let candidates = crate::codex_router::resume::selection::waiting_candidates(
            settings, &waiting, state, now,
        );
        for waiting in candidates {
            if self.thread_sources.is_known_subagent(&waiting.thread_id) {
                self.queue
                    .discard_waiting_entries(std::slice::from_ref(&waiting))?;
                if state.retry_after.remove(&waiting.waiting_id).is_some() {
                    self.store.save(state)?;
                }
                continue;
            }
            let mut tried = BTreeSet::new();
            loop {
                let Some(account) = self.queue.eligible_account(&waiting.thread_id)? else {
                    break;
                };
                if !tried.insert(account.clone()) {
                    break;
                }
                let cwd = match (self.workspace)(&waiting.thread_id)
                    .and_then(crate::codex_router::resume::workspace::validate)
                {
                    Ok(cwd) => cwd,
                    Err(_) => {
                        state.retry_after.insert(
                            waiting.waiting_id.clone(),
                            UnixMillis::new(now.get() + RETRY_DELAY_MILLIS),
                        );
                        self.store.save(state)?;
                        break;
                    }
                };
                state.retry_after.remove(&waiting.waiting_id);
                let thread = waiting.thread_id.clone();
                let id = uuid::Uuid::new_v4().to_string();
                let attempt = ResumeAttempt {
                    retry_waiting_id: WaitingId::for_attempt(&id),
                    id,
                    account,
                    waiting: waiting.clone(),
                    cwd,
                    phase: ResumePhase::Authorizing,
                    terminal: None,
                };
                state.attempts.insert(thread, attempt.clone());
                self.store.save(state)?;
                match self.authorize(state, &attempt.waiting.thread_id.clone(), &attempt, now)? {
                    AuthorizationOutcome::Launched | AuthorizationOutcome::Cancelled => {
                        return Ok(())
                    }
                    AuthorizationOutcome::Retry => {}
                }
            }
        }
        Ok(())
    }

    pub(in crate::codex_router::resume) fn authorize(
        &mut self,
        state: &mut super::ResumeState,
        thread: &crate::rotation::ThreadId,
        attempt: &ResumeAttempt,
        now: UnixMillis,
    ) -> Result<AuthorizationOutcome> {
        match self
            .queue
            .authorize(&attempt.waiting, &attempt.id, &attempt.account)?
        {
            crate::rotation::ResumeAuthorization::Acquired => {
                let settings = self.settings.clone();
                settings.update(|settings| {
                    let outcome = if settings.cancelled_threads().contains(thread) {
                        self.stage_unlaunched_cancel(state, thread, attempt, now)
                            .map(|()| AuthorizationOutcome::Cancelled)
                    } else {
                        state
                            .attempts
                            .get_mut(thread)
                            .expect("current attempt")
                            .phase = ResumePhase::Launching;
                        self.store
                            .save(state)
                            .and_then(|()| self.units.launch(attempt))
                            .map(|()| AuthorizationOutcome::Launched)
                    };
                    (outcome, false)
                })?
            }
            crate::rotation::ResumeAuthorization::Cancelled => {
                self.stage_unlaunched_cancel(state, thread, attempt, now)?;
                Ok(AuthorizationOutcome::Cancelled)
            }
            crate::rotation::ResumeAuthorization::Stale
            | crate::rotation::ResumeAuthorization::Lost => {
                self.stage_abandoned(state, thread)?;
                Ok(AuthorizationOutcome::Retry)
            }
        }
    }
}
