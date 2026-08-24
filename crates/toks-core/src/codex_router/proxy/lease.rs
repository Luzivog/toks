use std::sync::Arc;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::engine::{Engine, RouteTier};

pub(super) struct StreamLease {
    engine: Arc<Engine>,
    account: AccountId,
    thread: ThreadId,
    tier: RouteTier,
    continues: bool,
}

pub(super) struct ThreadAttachment {
    engine: Arc<Engine>,
    account: AccountId,
    thread: ThreadId,
}

impl StreamLease {
    pub fn open(
        engine: Arc<Engine>,
        account: &AccountId,
        thread: &ThreadId,
        resume_attempt: Option<&str>,
    ) -> anyhow::Result<Option<Self>> {
        let Some(tier) = engine.route_authorized(account, thread, resume_attempt)? else {
            return Ok(None);
        };
        Ok(Some(Self {
            engine,
            account: account.clone(),
            thread: thread.clone(),
            tier,
            continues: false,
        }))
    }

    pub fn tier(&self) -> RouteTier {
        self.tier
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
