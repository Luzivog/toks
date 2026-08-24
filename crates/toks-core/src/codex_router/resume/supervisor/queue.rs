use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::{ResumeAuthorization, ResumeTerminal, ThreadId, WaitingId, WaitingThread};

pub(in crate::codex_router::resume) trait ResumeQueue {
    fn eligible_account(&mut self, thread: &ThreadId) -> Result<Option<AccountId>>;
    fn waiting_threads(&mut self) -> Vec<WaitingThread>;
    fn discard_waiting_entries(&mut self, discarded: &[WaitingThread]) -> Result<()>;
    fn authorize(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        account: &AccountId,
    ) -> Result<ResumeAuthorization>;
    fn finish(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        terminal: ResumeTerminal,
        replacement: WaitingId,
    ) -> Result<Option<WaitingThread>>;
    fn forget(&mut self, waiting: &WaitingThread, attempt: &str) -> Result<()>;
}

impl ResumeQueue for crate::codex_router::proxy::RouterRuntimeHandle {
    fn eligible_account(&mut self, thread: &ThreadId) -> Result<Option<AccountId>> {
        crate::codex_router::proxy::RouterRuntimeHandle::eligible_account_for_thread(self, thread)
    }

    fn waiting_threads(&mut self) -> Vec<WaitingThread> {
        crate::codex_router::proxy::RouterRuntimeHandle::waiting_threads(self)
    }

    fn discard_waiting_entries(&mut self, discarded: &[WaitingThread]) -> Result<()> {
        crate::codex_router::proxy::RouterRuntimeHandle::discard_waiting_entries(self, discarded)
    }

    fn authorize(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        account: &AccountId,
    ) -> Result<ResumeAuthorization> {
        crate::codex_router::proxy::RouterRuntimeHandle::authorize_resume(
            self, waiting, attempt, account,
        )
    }

    fn finish(
        &mut self,
        waiting: &WaitingThread,
        attempt: &str,
        terminal: ResumeTerminal,
        replacement: WaitingId,
    ) -> Result<Option<WaitingThread>> {
        crate::codex_router::proxy::RouterRuntimeHandle::finish_resume(
            self,
            waiting,
            attempt,
            terminal,
            replacement,
        )
    }

    fn forget(&mut self, waiting: &WaitingThread, attempt: &str) -> Result<()> {
        crate::codex_router::proxy::RouterRuntimeHandle::forget_resume(self, waiting, attempt)
    }
}
