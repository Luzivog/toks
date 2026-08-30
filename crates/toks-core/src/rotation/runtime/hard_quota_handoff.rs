use crate::accounts::AccountId;
use crate::rotation::{BlockWindow, UsageLimitIncident};

use super::{RotationRuntime, ThreadId, UnixMillis, WorkerConnectionOwner};

pub(crate) struct DeliveredHardLimitHandoff<'a> {
    pub owner: Option<WorkerConnectionOwner>,
    pub account: &'a AccountId,
    pub thread: &'a ThreadId,
    pub window: BlockWindow,
    pub incident: UsageLimitIncident,
    pub queue_continuation: bool,
    pub at: UnixMillis,
}

impl RotationRuntime {
    pub(crate) fn delivered_hard_limit_handoff(&mut self, handoff: DeliveredHardLimitHandoff<'_>) {
        let DeliveredHardLimitHandoff {
            owner,
            account,
            thread,
            window,
            incident,
            queue_continuation,
            at,
        } = handoff;
        self.thread_blocked(account, thread, window, at);
        self.usage_limited(account, incident, at);
        match owner {
            Some(owner) => {
                self.connection_closed_by(owner, account, thread, at);
                self.thread_detached_by(owner, account, thread);
            }
            None => {
                self.connection_closed(account, thread, at);
                self.thread_detached(account, thread);
            }
        }
        if queue_continuation {
            self.waiting(thread, at);
        }
    }
}
