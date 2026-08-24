use std::collections::BTreeSet;

use anyhow::Result;

use super::{ResumeQueue, ResumeState, Supervisor, TaskUnits};

impl<Q: ResumeQueue, U: TaskUnits> Supervisor<Q, U> {
    pub(super) fn prune_known_subagents(&mut self) -> Result<()> {
        let discarded = self
            .queue
            .waiting_threads()
            .into_iter()
            .filter(|waiting| self.thread_sources.is_known_subagent(&waiting.thread_id))
            .collect::<Vec<_>>();
        self.queue.discard_waiting_entries(&discarded)
    }

    pub(super) fn prune_retries(&mut self, state: &mut ResumeState) -> Result<()> {
        let waiting = self.queue.waiting_threads();
        let live = waiting
            .iter()
            .map(|waiting| waiting.waiting_id.clone())
            .collect::<BTreeSet<_>>();
        let before = state.retry_after.len();
        state
            .retry_after
            .retain(|waiting, _| live.contains(waiting));
        if state.retry_after.len() != before {
            self.store.save(state)?;
        }
        Ok(())
    }
}
