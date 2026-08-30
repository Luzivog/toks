use std::sync::Arc;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, ThreadOverride, ThreadRequestSettings};

use super::engine::{AuthorizedRoute, Engine, RouteTier};

pub(super) struct StreamLease {
    engine: Arc<Engine>,
    account: AccountId,
    thread: ThreadId,
    route: AuthorizedRoute,
    continues: bool,
}

pub(super) struct ThreadAttachment {
    engine: Arc<Engine>,
    account: AccountId,
    thread: ThreadId,
}

impl StreamLease {
    #[cfg(test)]
    pub fn open(
        engine: Arc<Engine>,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> anyhow::Result<Option<Self>> {
        Self::open_with_settings(engine, account, thread, resume_attempt, None)
    }

    pub fn open_observed(
        engine: Arc<Engine>,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
        request_settings: &ThreadRequestSettings,
    ) -> anyhow::Result<Option<Self>> {
        Self::open_with_settings(
            engine,
            account,
            thread,
            resume_attempt,
            Some(request_settings),
        )
    }

    fn open_with_settings(
        engine: Arc<Engine>,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
        request_settings: Option<&ThreadRequestSettings>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(route) =
            engine.open_tracked_stream(account, thread, resume_attempt, request_settings)?
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            engine,
            account: account.clone(),
            thread: thread.clone(),
            route,
            continues: false,
        }))
    }

    pub fn tier(&self) -> RouteTier {
        self.route.tier()
    }

    pub fn request_override(&self) -> Option<&ThreadOverride> {
        self.route.request_override()
    }

    pub fn continue_after_response(&mut self) {
        self.continues = true;
    }
}

impl ThreadAttachment {
    pub fn open(
        engine: Arc<Engine>,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> anyhow::Result<Option<Self>> {
        if !engine.open_tracked_attachment(account, thread, resume_attempt)? {
            return Ok(None);
        }
        Ok(Some(Self {
            engine,
            account: account.clone(),
            thread: thread.clone(),
        }))
    }

    pub fn matches(&self, thread: &ThreadId) -> bool {
        &self.thread == thread
    }
}

impl Drop for StreamLease {
    fn drop(&mut self) {
        let (operation, result) = if self.continues {
            (
                "record response continuation",
                self.engine
                    .continue_tracked_stream(&self.account, &self.thread),
            )
        } else {
            (
                "record response completion",
                self.engine
                    .close_tracked_stream(&self.account, &self.thread),
            )
        };
        report_cleanup_error(operation, result);
    }
}

impl Drop for ThreadAttachment {
    fn drop(&mut self) {
        report_cleanup_error(
            "record thread detachment",
            self.engine
                .detach_tracked_attachment(&self.account, &self.thread),
        );
    }
}

fn report_cleanup_error(operation: &str, result: anyhow::Result<()>) {
    if let Err(error) = result {
        eprintln!("toks router failed to {operation}: {error:#}");
    }
}
