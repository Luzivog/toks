use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

use super::super::{waiting::is_canonical_uuid, ThreadId, WaitingId, WaitingThread};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResumeAuthorization {
    Acquired,
    Cancelled,
    Stale,
    Lost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResumeTerminal {
    Success,
    Failure,
    Cancelled,
    Discarded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResumeRoute {
    Unclaimed,
    Authorized(AccountId),
    Denied,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::rotation::runtime) struct ResumeAdmission {
    pub(in crate::rotation::runtime) attempt: String,
    pub(in crate::rotation::runtime) account: AccountId,
    pub(in crate::rotation::runtime) waiting: WaitingThread,
    #[serde(default)]
    pub(in crate::rotation::runtime) phase: ResumeAdmissionPhase,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(in crate::rotation::runtime) enum ResumeAdmissionPhase {
    #[default]
    Active,
    Finished {
        waiting_id: Option<WaitingId>,
    },
}

impl ResumeAdmission {
    pub(in crate::rotation::runtime) fn active_binding(&self) -> Option<(&AccountId, &ThreadId)> {
        matches!(self.phase, ResumeAdmissionPhase::Active)
            .then_some((&self.account, &self.waiting.thread_id))
    }

    pub(in crate::rotation::runtime) fn validate(
        &self,
        key: &WaitingId,
        queued: &[WaitingThread],
        active_threads: &mut std::collections::BTreeSet<ThreadId>,
        attempts: &mut std::collections::BTreeSet<String>,
        replacements: &mut std::collections::BTreeSet<WaitingId>,
    ) -> anyhow::Result<()> {
        anyhow::ensure!(
            key == &self.waiting.waiting_id,
            "resume admission key does not match waiting identity"
        );
        anyhow::ensure!(key.is_recognized(), "unrecognized resume waiting identity");
        anyhow::ensure!(
            is_canonical_uuid(&self.attempt),
            "resume attempt id is not a canonical UUID"
        );
        anyhow::ensure!(
            attempts.insert(self.attempt.clone()),
            "duplicate resume attempt"
        );
        match &self.phase {
            ResumeAdmissionPhase::Active => {
                anyhow::ensure!(
                    active_threads.insert(self.waiting.thread_id.clone()),
                    "duplicate active resume thread"
                );
                anyhow::ensure!(
                    !queued.iter().any(|entry| {
                        entry.thread_id == self.waiting.thread_id
                            || entry.waiting_id == self.waiting.waiting_id
                    }),
                    "active resume identity is also waiting"
                );
            }
            ResumeAdmissionPhase::Finished {
                waiting_id: Some(waiting_id),
            } => {
                anyhow::ensure!(
                    waiting_id.is_recognized()
                        && replacements.insert(waiting_id.clone())
                        && queued.iter().any(|entry| {
                            &entry.waiting_id == waiting_id
                                && entry.thread_id == self.waiting.thread_id
                        }),
                    "finished resume replacement is missing"
                );
            }
            ResumeAdmissionPhase::Finished { waiting_id: None } => {
                anyhow::ensure!(
                    !queued
                        .iter()
                        .any(|entry| entry.waiting_id == self.waiting.waiting_id),
                    "finished resume identity is reused by waiting entry"
                );
            }
        }
        Ok(())
    }
}
