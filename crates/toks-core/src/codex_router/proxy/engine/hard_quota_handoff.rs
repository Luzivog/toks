use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::{DeliveredHardLimitHandoff, ThreadId, UnixMillis, UsageLimitIncident};
use crate::storage::StoreUpdate;

use super::{quota::block_window, Engine};

impl Engine {
    pub(in crate::codex_router::proxy) fn commit_delivered_hard_limit(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        reset: Option<UnixMillis>,
        incident: UsageLimitIncident,
    ) -> Result<()> {
        debug_assert_eq!(incident.thread_id(), Some(thread));
        let at = UnixMillis::now();
        let window = block_window(account, reset);
        let queue_continuation = !self.thread_sources.is_known_subagent(thread);
        let owner = self.connection_owner;
        let mut inventory = self.inventory();
        self.runtime.update(|runtime| {
            runtime.delivered_hard_limit_handoff(DeliveredHardLimitHandoff {
                owner,
                account,
                thread,
                window,
                incident,
                queue_continuation,
                at,
            });
            StoreUpdate::Changed(())
        })?;
        if owner.is_some() {
            let stream_closed = inventory.stream_closed(account, thread);
            let attachment_closed = inventory.attachment_closed(account, thread);
            debug_assert!(stream_closed);
            debug_assert!(attachment_closed);
        }
        drop(inventory);
        self.task_activity.cancelled(thread);
        self.task_activity.attachment_closed(thread);
        Ok(())
    }
}
