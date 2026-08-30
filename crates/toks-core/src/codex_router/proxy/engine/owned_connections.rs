use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, ThreadRequestSettings, UnixMillis};
use crate::storage::StoreUpdate;

use super::{AuthorizedRoute, Engine};

impl Engine {
    pub(in crate::codex_router::proxy) fn open_tracked_stream(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
        request_settings: Option<&ThreadRequestSettings>,
    ) -> Result<Option<AuthorizedRoute>> {
        let Some(_) = self.connection_owner else {
            return self.route_request_authorized(
                account,
                thread,
                resume_attempt,
                request_settings,
            );
        };
        let mut inventory = self.inventory();
        let route =
            self.route_request_authorized(account, thread, resume_attempt, request_settings)?;
        if route.is_some() {
            inventory.stream_opened(account, thread);
        }
        Ok(route)
    }

    pub(in crate::codex_router::proxy) fn close_tracked_stream(
        &self,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<()> {
        self.finish_tracked_stream(account, thread, false)
    }

    pub(in crate::codex_router::proxy) fn continue_tracked_stream(
        &self,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<()> {
        self.finish_tracked_stream(account, thread, true)
    }

    pub(in crate::codex_router::proxy) fn open_tracked_attachment(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> Result<bool> {
        let Some(_) = self.connection_owner else {
            return self.attach_authorized(account, thread, resume_attempt);
        };
        let mut inventory = self.inventory();
        let attached = self.attach_authorized(account, thread, resume_attempt)?;
        if attached {
            inventory.attachment_opened(account, thread);
        }
        Ok(attached)
    }

    pub(in crate::codex_router::proxy) fn detach_tracked_attachment(
        &self,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<()> {
        let Some(_) = self.connection_owner else {
            return self.detach(account, thread);
        };
        let mut inventory = self.inventory();
        let attachment_closed = inventory.attachment_closed(account, thread);
        debug_assert!(attachment_closed);
        self.detach(account, thread)
    }

    pub(crate) fn reconcile_owned_connections(&self) -> Result<()> {
        let Some(owner) = self.connection_owner else {
            return Ok(());
        };
        let mut inventory = self.inventory();
        let reconciled = self.runtime.update(|runtime| {
            match runtime.reconcile_worker_connection_inventory(
                owner,
                &inventory,
                UnixMillis::now(),
            ) {
                Ok(changed) => StoreUpdate::from_changed(Ok(()), changed),
                Err(error) => StoreUpdate::Unchanged(Err(error)),
            }
        })?;
        reconciled?;
        inventory.continuations_published();
        Ok(())
    }

    fn finish_tracked_stream(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        continues: bool,
    ) -> Result<()> {
        let Some(_) = self.connection_owner else {
            return if continues {
                self.continue_response(account, thread)
            } else {
                self.close(account, thread)
            };
        };
        let mut inventory = self.inventory();
        if continues {
            let stream_continues = inventory.stream_continues(account, thread);
            debug_assert!(stream_continues);
            let published = self.continue_response(account, thread);
            if published.is_ok() {
                inventory.continuation_published(account, thread);
            }
            published
        } else {
            let stream_closed = inventory.stream_closed(account, thread);
            debug_assert!(stream_closed);
            self.close(account, thread)
        }
    }

    pub(super) fn inventory(
        &self,
    ) -> std::sync::MutexGuard<'_, crate::rotation::WorkerConnectionInventory> {
        self.connection_inventory
            .lock()
            .expect("router connection inventory poisoned")
    }
}
