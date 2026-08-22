use crate::accounts::AccountId;

use super::{
    RotationEventKind, RotationRuntime, RouterHealth, ThreadId, UnixMillis, WaitingThread,
};

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

    pub fn reset_connections(&mut self) -> bool {
        let mut changed = !self.attached_threads.is_empty();
        self.attached_threads.clear();
        for state in self.accounts.values_mut() {
            changed |= state.active_streams != 0;
            state.active_streams = 0;
        }
        changed
    }

    pub fn thread_attached(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        let attachment = self
            .attached_threads
            .entry(thread.clone())
            .or_insert_with(|| super::AttachedThread {
                account: account.clone(),
                connections: 0,
            });
        let moved = &attachment.account != account;
        if moved {
            attachment.account = account.clone();
            attachment.connections = 0;
        }
        attachment.connections = attachment.connections.saturating_add(1);
        let mut persisted_changed = false;
        for (candidate, state) in &mut self.accounts {
            if candidate != account {
                persisted_changed |= state.grandfathered_threads.remove(thread);
            }
        }
        persisted_changed
    }

    pub fn thread_detached(&mut self, account: &AccountId, thread: &ThreadId) -> bool {
        let Some(attachment) = self
            .attached_threads
            .get_mut(thread)
            .filter(|attachment| &attachment.account == account)
        else {
            return false;
        };
        attachment.connections = attachment.connections.saturating_sub(1);
        if attachment.connections == 0 {
            self.attached_threads.remove(thread);
        }
        false
    }

    pub fn block(
        &mut self,
        account: &AccountId,
        until: UnixMillis,
        reset_known: bool,
        at: UnixMillis,
    ) -> bool {
        let state = self.accounts.entry(account.clone()).or_default();
        if state.blocked_until == Some(until)
            && state.block_confirmed
            && state.block_reset_known == reset_known
            && state.quota_exhaustion.is_none()
            && state.grandfathered_threads.is_empty()
        {
            return false;
        }
        state.blocked_until = Some(until);
        state.block_confirmed = true;
        state.block_reset_known = reset_known;
        state.quota_exhaustion = None;
        state.grandfathered_threads.clear();
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
