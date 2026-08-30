use std::collections::BTreeMap;

use toks_core::{
    codex_router::{
        account_activation::SelectableModel, thread_lineage::ThreadLineage, RouterDeploymentStatus,
        RouterInstallStatus,
    },
    rotation::{ActiveTaskRow, RotationRuntime, RotationSettings, TaskActivity, ThreadId},
};

use super::super::remote_control_operations::RemoteControlUiState;

#[derive(Debug, Clone)]
pub(crate) struct RotationUiState {
    pub settings: RotationSettings,
    pub runtime: RotationRuntime,
    pub thread_titles: BTreeMap<ThreadId, String>,
    pub thread_lineage: BTreeMap<ThreadId, ThreadLineage>,
    pub selectable_models: Vec<SelectableModel>,
    pub install: RouterInstallStatus,
    pub deployment: RouterDeploymentStatus,
    pub error: Option<String>,
    pub busy: Option<&'static str>,
    pub remote: RemoteControlUiState,
    pub(super) activity: ActiveTaskProjection,
    pub(super) generation: u64,
}

#[derive(Debug, Clone, Default)]
pub(super) enum ActiveTaskProjection {
    Available(Vec<ActiveTaskRow>),
    #[default]
    Unavailable,
}

impl ActiveTaskProjection {
    pub(super) fn from_activity_at(
        activity: &TaskActivity,
        observed_at: toks_core::rotation::UnixMillis,
    ) -> Self {
        match activity.active_task_rows_at(observed_at) {
            Ok(rows) => Self::Available(rows),
            Err(_) => Self::Unavailable,
        }
    }

    pub(super) fn rows(&self) -> Option<&[ActiveTaskRow]> {
        match self {
            Self::Available(rows) => Some(rows),
            Self::Unavailable => None,
        }
    }
}

impl RotationUiState {
    pub(crate) fn active_task_rows(&self) -> Option<&[ActiveTaskRow]> {
        self.activity.rows()
    }

    pub(crate) fn active_task_count(
        &self,
        account: &toks_core::accounts::AccountId,
    ) -> Option<u32> {
        self.active_task_rows().map(|rows| {
            rows.iter()
                .filter(|row| &row.account_id == account)
                .count()
                .try_into()
                .unwrap_or(u32::MAX)
        })
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn set_active_task_rows(&mut self, rows: Vec<ActiveTaskRow>) {
        self.activity = ActiveTaskProjection::Available(rows);
    }

    #[cfg(feature = "test-support")]
    pub(crate) fn set_activity_unavailable(&mut self) {
        self.activity = ActiveTaskProjection::Unavailable;
    }
}

impl Default for RotationUiState {
    fn default() -> Self {
        Self {
            settings: RotationSettings::default(),
            runtime: RotationRuntime::default(),
            thread_titles: Default::default(),
            thread_lineage: Default::default(),
            selectable_models: Vec::new(),
            install: RouterInstallStatus {
                configured: false,
                service_installed: false,
                service_active: false,
                resume_active: false,
            },
            deployment: RouterDeploymentStatus::default(),
            error: None,
            busy: None,
            remote: Default::default(),
            activity: ActiveTaskProjection::Unavailable,
            generation: 0,
        }
    }
}
