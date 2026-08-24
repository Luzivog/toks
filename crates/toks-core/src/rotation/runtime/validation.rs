use std::collections::BTreeSet;

use anyhow::Result;

use super::RotationRuntime;

impl RotationRuntime {
    pub(in crate::rotation) fn normalize(&mut self) -> Result<()> {
        for state in self.accounts.values_mut() {
            state.auth.normalize_rejected_credentials();
            if state.quota_drain.is_none() && !state.block_confirmed {
                state.grandfathered_threads.clear();
                state.provisional_threads.clear();
                state.thread_usage.clear();
            }
        }
        self.trim_events();
        self.validate()
    }

    pub(in crate::rotation) fn validate(&self) -> Result<()> {
        let mut waiting_ids = BTreeSet::new();
        let mut waiting_threads = BTreeSet::new();
        for waiting in &self.waiting_threads {
            anyhow::ensure!(
                waiting.waiting_id.is_recognized(),
                "unrecognized waiting identity"
            );
            anyhow::ensure!(
                waiting_ids.insert(waiting.waiting_id.clone()),
                "duplicate waiting identity"
            );
            anyhow::ensure!(
                waiting_threads.insert(waiting.thread_id.clone()),
                "duplicate waiting thread"
            );
        }
        let mut active_threads = BTreeSet::new();
        let mut attempts = BTreeSet::new();
        let mut replacements = BTreeSet::new();
        for (key, admission) in &self.resume_admissions {
            admission.validate(
                key,
                &self.waiting_threads,
                &mut active_threads,
                &mut attempts,
                &mut replacements,
            )?;
            if let Some((account, thread)) = admission.active_binding() {
                anyhow::ensure!(
                    self.claim_thread_account(account, thread).is_ok(),
                    "active resume admission conflicts with live thread ownership"
                );
            }
        }
        Ok(())
    }
}
