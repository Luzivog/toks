use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;

use super::engine::EngineConfig;
use super::types::{LocalCredentials, SharedCredentials};
use super::{Engine, RouterRuntimeHandle};
use crate::accounts::AccountId;
use crate::rotation::{
    ResumeAuthorization, ResumeTerminal, ThreadId, WaitingId, WaitingThread, WorkerConnectionOwner,
};

impl RouterRuntimeHandle {
    pub fn discover() -> Result<Self> {
        let credentials: SharedCredentials = Arc::new(LocalCredentials);
        let engine = Engine::new(EngineConfig::discover(credentials.clone())?)?;
        Ok(Self {
            engine,
            credentials,
        })
    }

    pub(crate) fn discover_for_worker(generation: u64, instance_id: u64) -> Result<Self> {
        let credentials: SharedCredentials = Arc::new(LocalCredentials);
        let owner = WorkerConnectionOwner::new(generation, instance_id)
            .ok_or_else(|| anyhow::anyhow!("router worker identity must be nonzero"))?;
        let mut config = EngineConfig::discover(credentials.clone())?;
        config.connection_owner = Some(owner);
        let engine = Engine::new(config)?;
        Ok(Self {
            engine,
            credentials,
        })
    }

    pub fn eligible_account(&self) -> Result<Option<AccountId>> {
        self.engine.eligible_account()
    }

    pub(crate) fn eligible_account_for_thread(
        &self,
        thread: &ThreadId,
    ) -> Result<Option<AccountId>> {
        self.engine.eligible_account_for_thread(thread)
    }

    pub fn waiting_threads(&self) -> Vec<WaitingThread> {
        self.engine.waiting_threads()
    }

    pub(crate) fn discard_waiting_entries(&self, discarded: &[WaitingThread]) -> Result<()> {
        self.engine.discard_waiting_entries(discarded)
    }

    pub fn waiting_after_attempt(
        &self,
        waiting: &WaitingThread,
        replacement: WaitingId,
    ) -> Result<Option<WaitingThread>> {
        self.engine.waiting_after_attempt(waiting, replacement)
    }

    pub fn claim_waiting(&self, waiting: &WaitingThread, account: &AccountId) -> Result<bool> {
        self.engine.claim_waiting_entry(waiting, account)
    }

    pub(crate) fn authorize_resume(
        &self,
        waiting: &WaitingThread,
        attempt: &str,
        account: &AccountId,
    ) -> Result<ResumeAuthorization> {
        self.engine.authorize_resume(waiting, attempt, account)
    }

    pub(crate) fn finish_resume(
        &self,
        waiting: &WaitingThread,
        attempt: &str,
        terminal: ResumeTerminal,
        replacement: WaitingId,
    ) -> Result<Option<WaitingThread>> {
        self.engine
            .finish_resume(waiting, attempt, terminal, replacement)
    }

    pub(crate) fn forget_resume(&self, waiting: &WaitingThread, attempt: &str) -> Result<()> {
        self.engine.forget_resume(waiting, attempt)
    }

    pub(crate) fn reconcile_connection_owners(&self, surviving: &BTreeMap<u64, u64>) -> Result<()> {
        self.engine.reconcile_connection_owners(surviving)
    }
}
