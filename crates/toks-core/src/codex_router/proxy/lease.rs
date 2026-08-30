use std::sync::Arc;

use crate::accounts::AccountId;
use crate::rotation::{
    ThreadId, ThreadOverride, ThreadRequestSettings, UnixMillis, UsageLimitIncident,
};

use super::engine::{AuthorizedRoute, Engine, RouteTier};

pub(super) struct StreamLease {
    engine: Arc<Engine>,
    account: AccountId,
    thread: ThreadId,
    route: AuthorizedRoute,
    continues: bool,
    disarmed: bool,
}

pub(super) struct ThreadAttachment {
    engine: Arc<Engine>,
    account: AccountId,
    thread: ThreadId,
    disarmed: bool,
}

pub(super) struct TerminalOwnership {
    stream: StreamLease,
    attachment: ThreadAttachment,
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
            disarmed: false,
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
            disarmed: false,
        }))
    }

    pub fn matches(&self, thread: &ThreadId) -> bool {
        &self.thread == thread
    }
}

impl TerminalOwnership {
    pub fn take(
        stream: &mut Option<StreamLease>,
        attachment: &mut Option<ThreadAttachment>,
    ) -> Option<Self> {
        if stream.is_none() || attachment.is_none() {
            return None;
        }
        Some(Self {
            stream: stream.take().expect("checked stream ownership"),
            attachment: attachment.take().expect("checked attachment ownership"),
        })
    }

    pub fn commit_delivered_hard_limit(
        mut self,
        reset: Option<UnixMillis>,
        incident: UsageLimitIncident,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            Arc::ptr_eq(&self.stream.engine, &self.attachment.engine)
                && self.stream.account == self.attachment.account
                && self.stream.thread == self.attachment.thread,
            "terminal ownership does not describe one routed thread"
        );
        anyhow::ensure!(
            !self.stream.continues,
            "continued response cannot enter terminal quota handoff"
        );
        self.stream.engine.commit_delivered_hard_limit(
            &self.stream.account,
            &self.stream.thread,
            reset,
            incident,
        )?;
        self.stream.disarmed = true;
        self.attachment.disarmed = true;
        Ok(())
    }
}

impl Drop for StreamLease {
    fn drop(&mut self) {
        if self.disarmed {
            return;
        }
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
        if self.disarmed {
            return;
        }
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
