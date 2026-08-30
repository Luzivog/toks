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
        let route = if self.connection_owner.is_none() {
            self.route_request_authorized(account, thread, resume_attempt, request_settings)?
        } else {
            let mut inventory = self.inventory();
            let route =
                self.route_request_authorized(account, thread, resume_attempt, request_settings)?;
            if route.is_some() {
                inventory.stream_opened(account, thread);
            }
            route
        };
        if route.is_some() {
            self.task_activity.started(
                account,
                thread,
                request_settings.cloned().unwrap_or_default(),
                UnixMillis::now(),
            );
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
        let attached = if self.connection_owner.is_none() {
            self.attach_authorized(account, thread, resume_attempt)?
        } else {
            let mut inventory = self.inventory();
            let attached = self.attach_authorized(account, thread, resume_attempt)?;
            if attached {
                inventory.attachment_opened(account, thread);
            }
            attached
        };
        if attached {
            self.task_activity.attachment_opened(thread);
        }
        Ok(attached)
    }

    pub(in crate::codex_router::proxy) fn detach_tracked_attachment(
        &self,
        account: &AccountId,
        thread: &ThreadId,
    ) -> Result<()> {
        if self.connection_owner.is_some() {
            let mut inventory = self.inventory();
            let attachment_closed = inventory.attachment_closed(account, thread);
            debug_assert!(attachment_closed);
        }
        let result = self.detach(account, thread);
        self.task_activity.attachment_closed(thread);
        result
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
        });
        let result = match reconciled {
            Ok(result) => result.map_err(Into::into),
            Err(error) => Err(error),
        };
        if result.is_ok() {
            inventory.continuations_published();
        }
        drop(inventory);
        self.task_activity.publish_current();
        result
    }

    fn finish_tracked_stream(
        &self,
        account: &AccountId,
        thread: &ThreadId,
        continues: bool,
    ) -> Result<()> {
        let result = if self.connection_owner.is_none() {
            if continues {
                self.continue_response(account, thread)
            } else {
                self.close(account, thread)
            }
        } else {
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
        };
        if continues {
            self.task_activity.continues(thread);
        } else {
            self.task_activity.finished(thread);
        }
        result
    }

    pub(super) fn inventory(
        &self,
    ) -> std::sync::MutexGuard<'_, crate::rotation::WorkerConnectionInventory> {
        self.connection_inventory
            .lock()
            .expect("router connection inventory poisoned")
    }
}
