use std::time::Duration;

use gpui::{AppContext, Context};
use toks_core::remote_control::{self, RemoteControlSnapshot, RemoteDevices};

use crate::ToksApp;

mod actions;
mod state;
pub(crate) use actions::RemoteAction;
pub(crate) use state::{RemoteControlUiState, RemoteIssue, RemoteOperation, RemotePanel};

struct PollResult {
    snapshot: RemoteControlSnapshot,
    pairing_claimed: bool,
    paired_devices: Option<Vec<toks_core::remote_control::RemoteDevice>>,
}

pub(super) fn spawn(cx: &mut Context<ToksApp>) {
    cx.spawn(async move |this, cx| loop {
        let request = this
            .update(cx, |app, _| {
                app.rotation.remote.expire_pairing(app.now.timestamp());
                app.rotation.remote.busy.is_none().then(|| {
                    (
                        app.rotation.remote.generation(),
                        app.rotation.remote.pairing_poll(),
                        app.rotation.remote.snapshot.environment_id.clone(),
                    )
                })
            })
            .ok()
            .flatten();
        let pairing_active = request
            .as_ref()
            .is_some_and(|(_, pairing, _)| pairing.is_some());
        if let Some((generation, pairing, environment)) = request {
            let result = cx
                .background_spawn(async move { poll(pairing, environment).await })
                .await;
            if this
                .update(cx, |app, cx| {
                    apply_poll(&mut app.rotation.remote, generation, result);
                    cx.notify();
                })
                .is_err()
            {
                break;
            }
        }
        smol::Timer::after(if pairing_active {
            Duration::from_secs(2)
        } else {
            Duration::from_secs(10)
        })
        .await;
    })
    .detach();
}

async fn poll(
    pairing: Option<remote_control::RemotePairing>,
    environment: Option<String>,
) -> remote_control::RemoteControlResult<PollResult> {
    let snapshot = remote_control::status().await?;
    let pairing_claimed = match &pairing {
        Some(pairing) => remote_control::pairing_claimed(pairing).await?,
        None => false,
    };
    let paired_devices = if pairing_claimed {
        let environment = snapshot.environment_id.as_ref().or(environment.as_ref());
        match environment {
            Some(environment) => Some(remote_control::devices(environment).await?),
            None => None,
        }
    } else {
        None
    };
    Ok(PollResult {
        snapshot,
        pairing_claimed,
        paired_devices,
    })
}

fn apply_poll(
    state: &mut RemoteControlUiState,
    generation: u64,
    result: remote_control::RemoteControlResult<PollResult>,
) {
    if state.busy.is_some() || !state.accepts(generation) {
        return;
    }
    match result {
        Ok(result) => {
            state.apply_snapshot(result.snapshot);
            if result.pairing_claimed {
                state.pairing = None;
                state.panel = RemotePanel::Devices;
                if let Some(devices) = result.paired_devices {
                    state.snapshot.devices = RemoteDevices::Loaded(devices);
                }
            }
        }
        Err(failure) if state.issue.is_none() => state.fail(failure),
        Err(_) => {}
    }
}

#[cfg(test)]
#[path = "remote_control_operations/tests.rs"]
mod tests;
