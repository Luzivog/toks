use std::collections::BTreeMap;
use std::sync::Mutex;

use super::catalogue::Catalogue;
use super::types::SharedCredentials;
use crate::accounts::AccountId;
use crate::rotation::{
    ResumeAuthorization, ResumeTerminal, RotationRuntime, RotationSettingsStore, ThreadId,
    UnixMillis, WaitingId, WaitingThread, WorkerConnectionInventory, WorkerConnectionOwner,
};
use crate::storage::StoreUpdate;
use anyhow::Result;

mod construction;
pub(super) use construction::EngineConfig;
mod hard_quota_handoff;
mod owned_connections;
#[cfg(test)]
mod process_safety_tests;
mod quota;
mod reconciliation;
mod request_route;
mod runtime_writer;
mod selection;
mod task_activity;
#[cfg(test)]
mod task_activity_tests;
use crate::codex_router::thread_source::ThreadSourceStore;
pub(crate) use quota::{AttemptedTier, ResponseDelivery, SnapshotApplication, UsageLimitAction};
pub(in crate::codex_router::proxy) use request_route::AuthorizedRoute;
use runtime_writer::RuntimeWriter;
pub(super) use selection::RouteSelection;
use task_activity::TaskActivityPublisher;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RouteTier {
    Original,
    Fast,
    Standard,
}

pub(super) struct Engine {
    credentials: SharedCredentials,
    settings: RotationSettingsStore,
    runtime: RuntimeWriter,
    catalogue: Catalogue,
    connection_owner: Option<WorkerConnectionOwner>,
    connection_inventory: Mutex<WorkerConnectionInventory>,
    task_activity: TaskActivityPublisher,
    thread_sources: ThreadSourceStore,
    activation: crate::codex_router::account_activation::Store,
}
impl Engine {
    pub fn close(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| match self.connection_owner {
            Some(owner) => runtime.connection_closed_by(owner, account, thread, UnixMillis::now()),
            None => runtime.connection_closed(account, thread, UnixMillis::now()),
        })
    }

    pub fn continue_response(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| match self.connection_owner {
            Some(owner) => {
                runtime.connection_continues_by(owner, account, thread, UnixMillis::now())
            }
            None => runtime.connection_continues(account, thread, UnixMillis::now()),
        })
    }

    pub fn detach(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| match self.connection_owner {
            Some(owner) => runtime.thread_detached_by(owner, account, thread),
            None => runtime.thread_detached(account, thread),
        })
    }

    pub fn waiting_threads(&self) -> Vec<WaitingThread> {
        self.runtime
            .latest(|runtime| runtime.waiting_threads().to_vec())
            .unwrap_or_else(|_| {
                self.runtime
                    .cached(|runtime| runtime.waiting_threads().to_vec())
            })
    }

    pub fn discard_waiting_entries(&self, discarded: &[WaitingThread]) -> Result<()> {
        self.mutate(|runtime| runtime.discard_waiting_entries(discarded))
    }

    #[cfg(test)]
    pub fn claim_waiting(&self, thread: &ThreadId, account: &AccountId) -> Result<bool> {
        self.runtime.update(|runtime| {
            let claimed = runtime.resumed(thread, account, UnixMillis::now());
            StoreUpdate::from_changed(claimed, claimed)
        })
    }

    pub fn claim_waiting_entry(
        &self,
        waiting: &WaitingThread,
        account: &AccountId,
    ) -> Result<bool> {
        self.runtime.update(|runtime| {
            let claimed = runtime.resumed_waiting(waiting, account, UnixMillis::now());
            StoreUpdate::from_changed(claimed, claimed)
        })
    }

    pub fn waiting_after_attempt(
        &self,
        waiting: &WaitingThread,
        replacement: crate::rotation::WaitingId,
    ) -> Result<Option<WaitingThread>> {
        anyhow::ensure!(replacement.is_recognized(), "unrecognized waiting identity");
        self.runtime.update(|runtime| {
            let requeued = runtime.waiting_after_attempt(waiting, replacement, UnixMillis::now());
            let changed = requeued.is_some();
            StoreUpdate::from_changed(requeued, changed)
        })
    }

    pub fn authorize_resume(
        &self,
        waiting: &WaitingThread,
        attempt: &str,
        account: &AccountId,
    ) -> Result<ResumeAuthorization> {
        validate_resume_attempt(attempt)?;
        let discovered = self.credentials.account_ids();
        self.settings.update(|settings| {
            settings.reconcile(&discovered);
            let authorization = self.runtime.update(|runtime| {
                let authorization = runtime.authorize_resume(
                    settings,
                    &discovered,
                    waiting,
                    attempt,
                    account,
                    UnixMillis::now(),
                );
                StoreUpdate::from_changed(
                    authorization,
                    authorization == ResumeAuthorization::Acquired,
                )
            });
            StoreUpdate::Unchanged(authorization)
        })?
    }

    pub fn finish_resume(
        &self,
        waiting: &WaitingThread,
        attempt: &str,
        terminal: ResumeTerminal,
        replacement: WaitingId,
    ) -> Result<Option<WaitingThread>> {
        validate_resume_attempt(attempt)?;
        if terminal == ResumeTerminal::Failure {
            anyhow::ensure!(replacement.is_recognized(), "unrecognized waiting identity");
        }
        self.runtime.update(|runtime| {
            let queued =
                runtime.finish_resume(waiting, attempt, terminal, replacement, UnixMillis::now());
            StoreUpdate::Changed(queued)
        })
    }

    pub fn forget_resume(&self, waiting: &WaitingThread, attempt: &str) -> Result<()> {
        validate_resume_attempt(attempt)?;
        let forgotten = self.runtime.update(|runtime| {
            let forgotten = runtime.forget_resume(waiting, attempt);
            let changed = forgotten == Ok(true);
            StoreUpdate::from_changed(forgotten, changed)
        })?;
        forgotten.map_err(|()| anyhow::anyhow!("resume admission is not terminal"))?;
        Ok(())
    }

    #[cfg(test)]
    pub fn reset_connections(&self) -> Result<()> {
        self.mutate(|runtime| runtime.reset_connections(UnixMillis::now()))
    }

    pub fn reconcile_connection_owners(&self, surviving: &BTreeMap<u64, u64>) -> Result<()> {
        self.mutate(|runtime| runtime.reconcile_connection_owners(surviving))
    }

    fn mutate(&self, change: impl FnOnce(&mut RotationRuntime) -> bool) -> Result<()> {
        self.runtime
            .update(|runtime| StoreUpdate::from_changed((), change(runtime)))
    }
}
fn validate_resume_attempt(attempt: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(attempt)?;
    anyhow::ensure!(
        parsed.to_string() == attempt,
        "non-canonical resume attempt id"
    );
    Ok(())
}
