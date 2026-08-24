use toks_core::{
    codex_router::{RouterDeploymentStatus, RouterInstallStatus},
    rotation::{RotationRuntime, RotationSettings},
};

use super::RotationUiState;

impl Default for RotationUiState {
    fn default() -> Self {
        Self {
            settings: RotationSettings::default(),
            runtime: RotationRuntime::default(),
            install: RouterInstallStatus {
                configured: false,
                service_installed: false,
                service_active: false,
            },
            deployment: RouterDeploymentStatus::default(),
            error: None,
            busy: None,
            remote: Default::default(),
            generation: 0,
        }
    }
}
