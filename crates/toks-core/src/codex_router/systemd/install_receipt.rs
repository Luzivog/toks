use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

use super::plan::Action;

#[derive(Debug, Clone, Copy, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
#[serde(default)]
pub(super) struct PendingInstall {
    pub(super) restart_coordinator: bool,
    pub(super) restart_resume: bool,
}

impl PendingInstall {
    pub(super) fn record_changes(&mut self, changes: [bool; 4], retry_failed: bool) {
        self.restart_coordinator |= changes[0] || changes[2] || retry_failed;
        self.restart_resume |= changes[3];
    }

    pub(super) fn requires_action(self) -> bool {
        self.restart_coordinator || self.restart_resume
    }

    pub(super) fn completed(&mut self, action: Action) -> bool {
        let before = *self;
        match action {
            Action::StartCoordinator | Action::RestartCoordinator => {
                self.restart_coordinator = false;
            }
            Action::StartResume | Action::RestartResume => self.restart_resume = false,
            _ => {}
        }
        *self != before
    }
}

pub(super) fn load(path: &Path) -> PendingInstall {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or(PendingInstall {
            restart_coordinator: path.exists(),
            restart_resume: path.exists(),
        })
}

pub(super) fn save(path: &Path, pending: &PendingInstall) -> Result<()> {
    crate::rotation::write_private_atomic(
        path,
        &serde_json::to_vec(pending)?,
        "router install receipt",
    )
}
