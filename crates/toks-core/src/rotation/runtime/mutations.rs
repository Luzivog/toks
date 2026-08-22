use crate::accounts::AccountId;

use super::{
    RotationEventKind, RotationRuntime, RouterHealth, ThreadId, UnixMillis, WaitingThread,
};

impl RotationRuntime {
    pub fn heartbeat(&mut self, at: UnixMillis) {
        self.health = RouterHealth::Healthy;
        self.heartbeat_at = Some(at);
    }

    pub fn router_failed(&mut self, at: UnixMillis) {
        self.health = RouterHealth::Failed;
        self.push_event(at, RotationEventKind::RouterFailure);
    }

    pub fn connection_opened(&mut self, account: &AccountId, thread: &ThreadId, at: UnixMillis) {
        let state = self.accounts.entry(account.clone()).or_default();
        state.active_streams = state.active_streams.saturating_add(1);
        self.push_event(
            at,
            RotationEventKind::Routed {
                thread_id: thread.clone(),
                account_id: account.clone(),
            },
        );
    }

    pub fn connection_closed(&mut self, account: &AccountId) -> bool {
        let Some(state) = self.accounts.get_mut(account) else {
            return false;
        };
        let Some(count) = state.active_streams.checked_sub(1) else {
            return false;
        };
        state.active_streams = count;
        true
    }

    pub fn block(&mut self, account: &AccountId, until: UnixMillis, at: UnixMillis) -> bool {
        let state = self.accounts.entry(account.clone()).or_default();
        if state.blocked_until == Some(until) {
            return false;
        }
        state.blocked_until = Some(until);
        self.push_event(
            at,
            RotationEventKind::Blocked {
                account_id: account.clone(),
                until,
            },
        );
        true
    }

    pub fn auth_failed(&mut self, account: &AccountId, at: UnixMillis) -> bool {
        let state = self.accounts.entry(account.clone()).or_default();
        if std::mem::replace(&mut state.needs_sign_in, true) {
            return false;
        }
        self.push_event(
            at,
            RotationEventKind::AuthNeeded {
                account_id: account.clone(),
            },
        );
        true
    }

    pub fn sign_in_restored(&mut self, account: &AccountId) -> bool {
        self.accounts
            .get_mut(account)
            .is_some_and(|state| std::mem::replace(&mut state.needs_sign_in, false))
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
        if self
            .waiting_threads
            .iter()
            .any(|waiting| &waiting.thread_id == thread)
        {
            return false;
        }
        self.waiting_threads.push(WaitingThread {
            thread_id: thread.clone(),
            since: at,
        });
        self.push_event(
            at,
            RotationEventKind::Waiting {
                thread_id: thread.clone(),
            },
        );
        true
    }

    pub fn resumed(&mut self, thread: &ThreadId, account: &AccountId, at: UnixMillis) -> bool {
        let before = self.waiting_threads.len();
        self.waiting_threads
            .retain(|waiting| &waiting.thread_id != thread);
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
}
