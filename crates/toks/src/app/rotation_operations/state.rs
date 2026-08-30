use std::collections::BTreeMap;

use toks_core::{
    codex_router::{
        account_activation::SelectableModel, thread_lineage::ThreadLineage, RouterDeploymentStatus,
        RouterInstallStatus,
    },
    rotation::{RotationRuntime, RotationSettings, ThreadId},
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
    pub(super) generation: u64,
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
            generation: 0,
        }
    }
}
