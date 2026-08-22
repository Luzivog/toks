use gpui::{AppContext, Context};
use toks_core::remote_control::{self, RemoteControlSnapshot, RemoteDevices, RemotePairing};

use crate::ToksApp;

use super::{RemoteOperation, RemotePanel};

pub(crate) enum RemoteAction {
    Enable,
    Disable,
    Pair,
    LoadDevices,
    Revoke(String),
}

enum Outcome {
    Snapshot(RemoteControlSnapshot),
    Pairing(RemotePairing),
    Devices(Vec<toks_core::remote_control::RemoteDevice>),
}

impl ToksApp {
    pub(crate) fn run_remote_action(&mut self, action: RemoteAction, cx: &mut Context<Self>) {
        let operation = operation(&action);
        let Some(generation) = self.rotation.remote.begin(operation) else {
            return;
        };
        let environment = self.rotation.remote.snapshot.environment_id.clone();
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move { perform(action, environment.as_deref()).await })
                .await;
            let _ = this.update(cx, |app, cx| {
                app.finish_remote_action(generation, result);
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_remote_action(
        &mut self,
        generation: u64,
        result: remote_control::RemoteControlResult<Outcome>,
    ) {
        if !self.rotation.remote.accepts(generation) {
            return;
        }
        let operation = self.rotation.remote.busy.clone();
        self.rotation.remote.finish(generation);
        match result {
            Ok(Outcome::Snapshot(snapshot)) => {
                self.rotation.remote.apply_snapshot(snapshot);
                self.rotation.remote.panel = RemotePanel::Summary;
            }
            Ok(Outcome::Pairing(pairing)) => {
                self.rotation.remote.snapshot.environment_id = Some(pairing.environment_id.clone());
                self.rotation.remote.pairing = Some(pairing);
                self.rotation.remote.panel = RemotePanel::Pairing;
            }
            Ok(Outcome::Devices(devices)) => {
                self.rotation.remote.snapshot.devices = RemoteDevices::Loaded(devices);
                self.rotation.remote.pending_revoke = None;
                self.rotation.remote.panel = RemotePanel::Devices;
            }
            Err(failure) => {
                if matches!(
                    operation,
                    Some(RemoteOperation::LoadingDevices | RemoteOperation::Revoking(_))
                ) {
                    self.rotation.remote.snapshot.devices =
                        RemoteDevices::Failed(failure.detail.clone());
                }
                self.rotation.remote.fail(failure);
            }
        }
    }
}

fn operation(action: &RemoteAction) -> RemoteOperation {
    match action {
        RemoteAction::Enable => RemoteOperation::Enabling,
        RemoteAction::Disable => RemoteOperation::Disabling,
        RemoteAction::Pair => RemoteOperation::Pairing,
        RemoteAction::LoadDevices => RemoteOperation::LoadingDevices,
        RemoteAction::Revoke(client) => RemoteOperation::Revoking(client.clone()),
    }
}

async fn perform(
    action: RemoteAction,
    environment: Option<&str>,
) -> remote_control::RemoteControlResult<Outcome> {
    match action {
        RemoteAction::Enable => remote_control::enable().await.map(Outcome::Snapshot),
        RemoteAction::Disable => remote_control::disable().await.map(Outcome::Snapshot),
        RemoteAction::Pair => remote_control::start_pairing().await.map(Outcome::Pairing),
        RemoteAction::LoadDevices => remote_control::devices(required_environment(environment)?)
            .await
            .map(Outcome::Devices),
        RemoteAction::Revoke(client) => {
            let environment = required_environment(environment)?;
            remote_control::revoke_device(environment, &client).await?;
            remote_control::devices(environment)
                .await
                .map(Outcome::Devices)
        }
    }
}

fn required_environment(environment: Option<&str>) -> remote_control::RemoteControlResult<&str> {
    environment.ok_or_else(|| {
        anyhow::anyhow!("Pair a device before managing Remote Control access").into()
    })
}
