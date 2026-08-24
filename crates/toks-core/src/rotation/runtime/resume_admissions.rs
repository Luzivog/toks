use crate::accounts::AccountId;

use super::{RotationEventKind, RotationRuntime, UnixMillis, WaitingId, WaitingThread};

mod authorization;
mod types;
pub(in crate::rotation::runtime) use types::ResumeAdmission;
use types::ResumeAdmissionPhase;
pub(crate) use types::{ResumeAuthorization, ResumeRoute, ResumeTerminal};

impl RotationRuntime {
    pub(crate) fn discard_waiting_entries(&mut self, discarded: &[WaitingThread]) -> bool {
        if discarded.is_empty() {
            return false;
        }
        let mut removed = std::collections::BTreeSet::new();
        self.waiting_threads.retain(|current| {
            let discard = discarded.iter().any(|candidate| candidate == current);
            if discard {
                removed.insert(current.waiting_id.clone());
            }
            !discard
        });
        if removed.is_empty() {
            return false;
        }
        self.resume_admissions.retain(|_, admission| {
            !matches!(
                &admission.phase,
                ResumeAdmissionPhase::Finished {
                    waiting_id: Some(waiting_id)
                } if removed.contains(waiting_id)
            )
        });
        true
    }

    pub(crate) fn resume_attempt_binding(
        &self,
        attempt: &str,
    ) -> Option<(AccountId, super::ThreadId)> {
        let mut bindings = self
            .resume_admissions
            .values()
            .filter(|admission| {
                admission.attempt == attempt && admission.phase == ResumeAdmissionPhase::Active
            })
            .map(|admission| {
                (
                    admission.account.clone(),
                    admission.waiting.thread_id.clone(),
                )
            });
        let binding = bindings.next()?;
        bindings.next().is_none().then_some(binding)
    }
    pub(crate) fn resume_route_authorized(
        &self,
        thread: &super::ThreadId,
        attempt: Option<&str>,
        account: &AccountId,
    ) -> bool {
        match self.resume_route(thread, attempt) {
            ResumeRoute::Unclaimed => true,
            ResumeRoute::Authorized(expected) => &expected == account,
            ResumeRoute::Denied => false,
        }
    }
    pub(crate) fn resume_route(
        &self,
        thread: &super::ThreadId,
        attempt: Option<&str>,
    ) -> ResumeRoute {
        self.resume_admissions
            .values()
            .find(|admission| admission.waiting.thread_id == *thread)
            .map_or_else(
                || {
                    if attempt.is_some() {
                        ResumeRoute::Denied
                    } else {
                        ResumeRoute::Unclaimed
                    }
                },
                |admission| match admission.phase {
                    ResumeAdmissionPhase::Active if Some(admission.attempt.as_str()) == attempt => {
                        ResumeRoute::Authorized(admission.account.clone())
                    }
                    ResumeAdmissionPhase::Active | ResumeAdmissionPhase::Finished { .. } => {
                        ResumeRoute::Denied
                    }
                },
            )
    }
    pub(crate) fn resume_in_progress(&self, thread: &super::ThreadId) -> bool {
        !matches!(self.resume_route(thread, None), ResumeRoute::Unclaimed)
    }
    pub(crate) fn finish_resume(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        terminal: ResumeTerminal,
        replacement: WaitingId,
        at: UnixMillis,
    ) -> Option<WaitingThread> {
        let admission = self.resume_admissions.get(&waiting.waiting_id)?;
        if admission.attempt != attempt {
            return None;
        }
        if let ResumeAdmissionPhase::Finished { waiting_id } = &admission.phase {
            return waiting_id.as_ref().and_then(|id| {
                self.waiting_threads
                    .iter()
                    .find(|current| &current.waiting_id == id)
                    .cloned()
            });
        }
        let queued = match terminal {
            ResumeTerminal::Success | ResumeTerminal::Discarded => None,
            ResumeTerminal::Failure => self
                .waiting_threads
                .iter()
                .find(|current| current.thread_id == waiting.thread_id)
                .cloned()
                .or_else(|| {
                    Some(WaitingThread::with_id(
                        replacement,
                        waiting.thread_id.clone(),
                        at,
                    ))
                }),
            ResumeTerminal::Cancelled => self
                .waiting_threads
                .iter()
                .find(|current| current.thread_id == waiting.thread_id)
                .cloned()
                .or_else(|| Some(waiting.clone())),
        };
        let newly_queued = queued.as_ref().is_some_and(|queued| {
            !self
                .waiting_threads
                .iter()
                .any(|current| current.waiting_id == queued.waiting_id)
        });
        if let Some(queued) = &queued {
            if newly_queued {
                self.waiting_threads.push(queued.clone());
            }
        }
        self.resume_admissions
            .get_mut(&waiting.waiting_id)
            .expect("admission remains current")
            .phase = ResumeAdmissionPhase::Finished {
            waiting_id: queued.as_ref().map(|entry| entry.waiting_id.clone()),
        };
        if newly_queued {
            self.push_event(
                at,
                RotationEventKind::Waiting {
                    thread_id: waiting.thread_id.clone(),
                },
            );
        }
        queued
    }

    pub(crate) fn forget_resume(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
    ) -> Result<bool, ()> {
        let Some(admission) = self.resume_admissions.get(&waiting.waiting_id) else {
            return Ok(false);
        };
        if admission.attempt != attempt
            || !matches!(admission.phase, ResumeAdmissionPhase::Finished { .. })
        {
            return Err(());
        }
        self.resume_admissions.remove(&waiting.waiting_id);
        Ok(true)
    }
}
