use toks_core::remote_control::{
    RemoteControlFailure, RemoteControlFailureKind, RemoteControlSnapshot, RemoteDevices,
    RemotePairing,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemotePanel {
    Summary,
    Pairing,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RemoteOperation {
    Enabling,
    Reconnecting,
    Disabling,
    Pairing,
    Revoking(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RemoteIssue {
    pub kind: RemoteControlFailureKind,
}

#[derive(Clone, Debug)]
pub(crate) struct RemoteControlUiState {
    pub snapshot: RemoteControlSnapshot,
    pub panel: RemotePanel,
    pub pairing: Option<RemotePairing>,
    pub action_issue: Option<RemoteIssue>,
    pub status_issue: Option<RemoteIssue>,
    pub pending_revoke: Option<String>,
    pub busy: Option<RemoteOperation>,
    generation: u64,
}

impl Default for RemoteControlUiState {
    fn default() -> Self {
        Self {
            snapshot: Default::default(),
            panel: RemotePanel::Summary,
            pairing: None,
            action_issue: None,
            status_issue: None,
            pending_revoke: None,
            busy: None,
            generation: 0,
        }
    }
}

impl RemoteControlUiState {
    pub fn begin(&mut self, operation: RemoteOperation) -> Option<u64> {
        if self.busy.is_some() {
            return None;
        }
        self.generation = self.generation.wrapping_add(1);
        self.busy = Some(operation);
        self.action_issue = None;
        Some(self.generation)
    }

    pub fn accepts(&self, generation: u64) -> bool {
        self.generation == generation
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn finish(&mut self, generation: u64) {
        if self.accepts(generation) {
            self.busy = None;
        }
    }

    pub fn apply_snapshot(&mut self, mut snapshot: RemoteControlSnapshot) {
        let status_changed = snapshot.connection.status != self.snapshot.connection.status;
        if snapshot.environment_id.is_none() {
            snapshot
                .environment_id
                .clone_from(&self.snapshot.environment_id);
        }
        if matches!(snapshot.devices, RemoteDevices::NotLoaded)
            && snapshot.environment_id == self.snapshot.environment_id
        {
            snapshot.devices.clone_from(&self.snapshot.devices);
        }
        self.snapshot = snapshot;
        self.status_issue = None;
        if status_changed {
            self.action_issue = None;
        }
    }

    pub fn fail_action(&mut self, failure: RemoteControlFailure) {
        self.action_issue = Some(RemoteIssue { kind: failure.kind });
    }

    pub fn fail_status(&mut self, failure: RemoteControlFailure) {
        self.status_issue = Some(RemoteIssue { kind: failure.kind });
    }

    pub fn expire_pairing(&mut self, now_seconds: i64) {
        if self
            .pairing
            .as_ref()
            .is_some_and(|pairing| pairing.has_expired(now_seconds))
        {
            self.pairing = None;
            self.panel = RemotePanel::Summary;
        }
    }

    pub fn pairing_poll(&self) -> Option<RemotePairing> {
        (self.busy.is_none() && self.panel == RemotePanel::Pairing)
            .then(|| self.pairing.clone())
            .flatten()
    }

    pub fn confirm_revoke(&mut self, client_id: String) {
        if self.busy.is_none() {
            self.pending_revoke = Some(client_id);
        }
    }

    pub fn cancel_revoke(&mut self) {
        self.pending_revoke = None;
    }
}

#[cfg(test)]
mod tests {
    use super::{RemoteControlUiState, RemoteOperation};

    #[test]
    fn duplicate_operations_and_stale_results_are_rejected() {
        let mut state = RemoteControlUiState::default();
        let first = state.begin(RemoteOperation::Enabling).unwrap();
        assert!(state.begin(RemoteOperation::Disabling).is_none());
        state.finish(first);
        let second = state.begin(RemoteOperation::Disabling).unwrap();
        state.finish(first);
        assert!(state.busy.is_some());
        state.finish(second);
        assert!(state.busy.is_none());
    }
}
