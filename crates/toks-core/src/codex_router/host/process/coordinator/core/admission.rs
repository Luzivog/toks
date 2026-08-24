use super::Coordinator;

impl Coordinator {
    pub(crate) fn accepts_clients(&self) -> bool {
        self.admission_block().is_none()
    }

    /// Why the coordinator is refusing clients, or `None` when it is admitting.
    ///
    /// Refusing is indistinguishable from a hang at the socket — clients simply
    /// wait in the backlog forever — so the reason has to be reportable.
    pub(crate) fn admission_block(&self) -> Option<&'static str> {
        if !self.pending.has_capacity() {
            return Some("handoff capacity is exhausted");
        }
        let Some(generation) = self.active else {
            return Some("no generation is active");
        };
        match self.workers.get(&generation) {
            None => Some("the active generation has no registered worker"),
            Some(worker) if !worker.ready => Some("the active worker is not ready"),
            Some(worker) if !worker.accepting => Some("the active worker is not accepting"),
            Some(_) => None,
        }
    }
}
