use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, WaitingId, WaitingThread};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::codex_router::resume) enum ResumePhase {
    Authorizing,
    Launching,
    Running,
    Cleaning,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::codex_router::resume) enum ResumeTerminalState {
    Success,
    Failure,
    Cancelled,
    Discarded,
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::codex_router::resume) struct ResumeAttempt {
    pub(in crate::codex_router::resume) id: String,
    pub(in crate::codex_router::resume) account: AccountId,
    pub(in crate::codex_router::resume) waiting: WaitingThread,
    pub(in crate::codex_router::resume) cwd: PathBuf,
    pub(in crate::codex_router::resume) phase: ResumePhase,
    pub(in crate::codex_router::resume) retry_waiting_id: WaitingId,
    pub(in crate::codex_router::resume) terminal: Option<ResumeTerminalState>,
}

impl<'de> Deserialize<'de> for ResumeAttempt {
    fn deserialize<D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Stored {
            id: String,
            account: AccountId,
            waiting: WaitingThread,
            cwd: PathBuf,
            phase: ResumePhase,
            #[serde(default)]
            retry_waiting_id: Option<WaitingId>,
            #[serde(default)]
            terminal: Option<ResumeTerminalState>,
        }
        let stored = Stored::deserialize(deserializer)?;
        Ok(Self {
            retry_waiting_id: stored
                .retry_waiting_id
                .unwrap_or_else(|| WaitingId::for_attempt(&stored.id)),
            id: stored.id,
            account: stored.account,
            waiting: stored.waiting,
            cwd: stored.cwd,
            phase: stored.phase,
            terminal: stored.terminal,
        })
    }
}

impl ResumeAttempt {
    pub(super) fn validate(&self, thread: &ThreadId) -> anyhow::Result<()> {
        super::validate_attempt_id(&self.id)?;
        anyhow::ensure!(
            &self.waiting.thread_id == thread,
            "resume attempt thread key does not match waiting thread"
        );
        anyhow::ensure!(self.cwd.is_absolute(), "resume workspace is not absolute");
        anyhow::ensure!(
            self.retry_waiting_id == WaitingId::for_attempt(&self.id),
            "resume retry identity does not match attempt"
        );
        let terminal_phase = self.phase == ResumePhase::Cleaning;
        anyhow::ensure!(
            terminal_phase == self.terminal.is_some(),
            "resume phase and terminal state are inconsistent"
        );
        Ok(())
    }
}
