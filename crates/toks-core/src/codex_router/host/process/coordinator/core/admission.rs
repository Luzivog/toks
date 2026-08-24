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
        if !self.workers.contains(generation) {
            Some("the active generation has no registered worker")
        } else if !self.workers.is_ready(generation) {
            Some("the active worker is not ready")
        } else if !self.workers.is_accepting(generation) {
            Some("the active worker is not accepting")
        } else {
            None
        }
    }
}
