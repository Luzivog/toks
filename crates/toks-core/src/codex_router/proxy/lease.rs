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
            engine.route_request_authorized(account, thread, resume_attempt, request_settings)?
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
        if !engine.attach_authorized(account, thread, resume_attempt)? {
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
        if self.continues {
            let _ = self.engine.continue_response(&self.account, &self.thread);
        } else {
            let _ = self.engine.close(&self.account, &self.thread);
        }
    }
}

impl Drop for ThreadAttachment {
    fn drop(&mut self) {
        let _ = self.engine.detach(&self.account, &self.thread);
    }
}
