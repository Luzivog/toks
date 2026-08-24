use crate::accounts::AccountId;

use super::{
    RotationEventKind, RotationRuntime, RouterHealth, ThreadId, UnixMillis, WaitingThread,
};

mod attachments;
mod auth;
mod limits;
mod quota;

impl RotationRuntime {
    pub fn heartbeat(&mut self, at: UnixMillis) {
        self.health = RouterHealth::Healthy;
        self.heartbeat_at = Some(at);
    }

    pub fn router_failed(&mut self, at: UnixMillis) {
        self.health = RouterHealth::Failed;
        self.push_event(at, RotationEventKind::RouterFailure);
    }

    pub fn rotated(&mut self, thread: &ThreadId, from: &AccountId, to: &AccountId, at: UnixMillis) {
        self.push_event(
            at,
            RotationEventKind::Rotated {
                thread_id: thread.clone(),
                from: from.clone(),
                to: to.clone(),
            },
        );
    }

    pub fn waiting(&mut self, thread: &ThreadId, at: UnixMillis) -> bool {
        if self.resume_in_progress(thread) {
            return false;
        }
        if self
            .waiting_threads
            .iter()
            .any(|waiting| &waiting.thread_id == thread)
        {
            return false;
        }
        self.waiting_threads
            .push(WaitingThread::new(thread.clone(), at));
        self.push_event(
            at,
            RotationEventKind::Waiting {
                thread_id: thread.clone(),
            },
        );
        true
    }

    pub fn resumed(&mut self, thread: &ThreadId, account: &AccountId, at: UnixMillis) -> bool {
        self.resumed_matching(thread, None, account, at)
    }

    pub(crate) fn resumed_waiting(
        &mut self,
        waiting: &WaitingThread,
        account: &AccountId,
        at: UnixMillis,
    ) -> bool {
        self.resumed_matching(&waiting.thread_id, Some(&waiting.waiting_id), account, at)
    }

    fn resumed_matching(
        &mut self,
        thread: &ThreadId,
        waiting_id: Option<&super::WaitingId>,
        account: &AccountId,
        at: UnixMillis,
    ) -> bool {
        if self.resume_in_progress(thread) {
            return false;
        }
        let before = self.waiting_threads.len();
        self.waiting_threads.retain(|waiting| {
            &waiting.thread_id != thread
                || waiting_id.is_some_and(|waiting_id| &waiting.waiting_id != waiting_id)
        });
        if self.waiting_threads.len() == before {
            return false;
        }
        self.push_event(
            at,
            RotationEventKind::Resumed {
                thread_id: thread.clone(),
                account_id: account.clone(),
            },
        );
        true
    }

    pub(crate) fn waiting_after_attempt(
        &mut self,
        waiting: &WaitingThread,
        replacement: super::WaitingId,
        at: UnixMillis,
    ) -> Option<WaitingThread> {
        if let Some(current) = self
            .waiting_threads
            .iter_mut()
            .find(|current| current.thread_id == waiting.thread_id)
        {
            if current.waiting_id == replacement {
                return Some(current.clone());
            }
            if current.waiting_id != waiting.waiting_id {
                return None;
            }
            *current = WaitingThread::with_id(replacement, waiting.thread_id.clone(), at);
            return Some(current.clone());
        }
        let queued = WaitingThread::with_id(replacement, waiting.thread_id.clone(), at);
        self.waiting_threads.push(queued.clone());
        self.push_event(
            at,
            RotationEventKind::Waiting {
                thread_id: waiting.thread_id.clone(),
            },
        );
        Some(queued)
    }
}
