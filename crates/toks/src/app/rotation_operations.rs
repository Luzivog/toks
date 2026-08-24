use std::time::Duration;

use anyhow::Result;
use gpui::{AppContext, Context};
use toks_core::{
    accounts::AccountId,
    codex_router::{RouterDeploymentStatus, RouterInstallStatus},
    rotation::{RotationRuntime, RotationSettings, ThreadId},
    Provider,
};

use super::remote_control_operations::RemoteControlUiState;
use crate::ToksApp;

mod io;
mod state;
use io::{change_settings, load_rotation, run_service_action, LoadedRotation};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RotationServiceAction {
    Enable,
    Disable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SettingsAction {
    Include(AccountId, bool),
    MoveAccount(AccountId, usize),
    Cancel(ThreadId),
    MoveWaiting(ThreadId, usize),
}

#[derive(Debug, Clone)]
pub(crate) struct RotationUiState {
    pub settings: RotationSettings,
    pub runtime: RotationRuntime,
    pub install: RouterInstallStatus,
    pub deployment: RouterDeploymentStatus,
    pub error: Option<String>,
    pub busy: Option<&'static str>,
    pub remote: RemoteControlUiState,
    generation: u64,
}

pub(super) fn spawn(cx: &mut Context<ToksApp>) {
    super::remote_control_operations::spawn(cx);
    cx.spawn(async move |this, cx| loop {
        let request = this
            .update(cx, |app, _| {
                app.rotation.busy.is_none().then(|| {
                    (
                        app.rotation.generation,
                        app.limits_loaded.then(|| app.codex_rotation_account_ids()),
                    )
                })
            })
            .ok()
            .flatten();
        if let Some((generation, accounts)) = request {
            let loaded = cx
                .background_spawn(async move { load_rotation(accounts.as_deref()) })
                .await;
            if this
                .update(cx, |app, cx| {
                    if app.rotation.generation == generation && app.rotation.busy.is_none() {
                        app.apply_rotation_poll(loaded);
                        cx.notify();
                    }
                })
                .is_err()
            {
                break;
            }
        }
        smol::Timer::after(Duration::from_secs(2)).await;
    })
    .detach();
}

impl ToksApp {
    pub(crate) fn change_rotation_settings(
        &mut self,
        action: SettingsAction,
        cx: &mut Context<Self>,
    ) {
        if self.rotation.busy.is_some() {
            return;
        }
        let accounts = self
            .limits_loaded
            .then(|| self.codex_rotation_account_ids());
        let generation = self.begin_rotation_work("Saving changes");
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { change_settings(action, accounts.as_deref()) })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_rotation_work(generation, result);
                cx.notify();
            });
        })
        .detach();
    }

    pub(crate) fn run_rotation_service_action(
        &mut self,
        action: RotationServiceAction,
        cx: &mut Context<Self>,
    ) {
        if self.rotation.busy.is_some() {
            return;
        }
        let accounts = self
            .limits_loaded
            .then(|| self.codex_rotation_account_ids());
        let generation = self.begin_rotation_work(match action {
            RotationServiceAction::Enable => "Enabling routing",
            RotationServiceAction::Disable => "Disabling routing",
        });
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    run_service_action(action)?;
                    load_rotation(accounts.as_deref())
                })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_rotation_work(generation, result);
                cx.notify();
            });
        })
        .detach();
    }

    fn codex_rotation_account_ids(&self) -> Vec<AccountId> {
        self.limits
            .iter()
            .filter(|snapshot| snapshot.provider == Provider::Codex)
            .map(|snapshot| snapshot.account.id.clone())
            .collect()
    }

    fn begin_rotation_work(&mut self, label: &'static str) -> u64 {
        self.rotation.generation = self.rotation.generation.wrapping_add(1);
        self.rotation.busy = Some(label);
        self.rotation.error = None;
        self.rotation.generation
    }

    fn finish_rotation_work(&mut self, generation: u64, result: Result<LoadedRotation>) {
        if self.rotation.generation != generation {
            return;
        }
        self.rotation.busy = None;
        self.apply_rotation_result(result);
    }

    fn apply_rotation_result(&mut self, result: Result<LoadedRotation>) {
        match result {
            Ok(loaded) => {
                self.rotation.settings = loaded.settings;
                self.rotation.runtime = loaded.runtime;
                self.rotation.install = loaded.install;
                self.rotation.deployment = loaded.deployment;
                self.rotation.error = None;
            }
            Err(error) => self.rotation.error = Some(error.to_string()),
        }
    }

    fn apply_rotation_poll(&mut self, result: Result<LoadedRotation>) {
        match result {
            Ok(loaded) => {
                self.rotation.settings = loaded.settings;
                self.rotation.runtime = loaded.runtime;
                self.rotation.install = loaded.install;
                self.rotation.deployment = loaded.deployment;
            }
            Err(error) if self.rotation.error.is_none() => {
                self.rotation.error = Some(error.to_string());
            }
            Err(_) => {}
        }
    }
}
