mod commands;
mod devices;
mod rpc;
mod store;
mod types;
mod wire;

use std::path::PathBuf;

use anyhow::{Context, Result};
use serde::Deserialize;
use serde_json::json;

use wire::StatusResponse;

pub use types::{
    RemoteConnection, RemoteConnectionStatus, RemoteControlFailure, RemoteControlFailureKind,
    RemoteControlSnapshot, RemoteDevice, RemoteDevices, RemotePairing,
};

pub type RemoteControlResult<T> = std::result::Result<T, RemoteControlFailure>;

pub async fn status() -> RemoteControlResult<RemoteControlSnapshot> {
    status_inner().await.map_err(Into::into)
}

async fn status_inner() -> Result<RemoteControlSnapshot> {
    let account = control_account_id();
    let stored_environment = account
        .as_ref()
        .map(store::environment)
        .transpose()?
        .flatten();
    let socket = socket_path()?;
    if !socket.exists() {
        return Ok(RemoteControlSnapshot {
            environment_id: stored_environment,
            ..Default::default()
        });
    }
    let response: StatusResponse = rpc::request(&socket, "remoteControl/status/read", None).await?;
    if let (Some(account), Some(environment)) = (&account, &response.environment_id) {
        store::remember(account, environment)?;
    }
    let environment_id = response.environment_id.or(stored_environment);
    Ok(RemoteControlSnapshot {
        connection: RemoteConnection {
            status: response.status.into(),
            server_name: Some(response.server_name),
        },
        environment_id,
        devices: RemoteDevices::NotLoaded,
    })
}

pub async fn enable() -> RemoteControlResult<RemoteControlSnapshot> {
    enable_inner().await.map_err(Into::into)
}

async fn enable_inner() -> Result<RemoteControlSnapshot> {
    let (connection, environment_id) = commands::enable().await?;
    if let (Some(account), Some(environment)) = (control_account_id(), &environment_id) {
        store::remember(&account, environment)?;
    }
    Ok(RemoteControlSnapshot {
        connection,
        environment_id,
        devices: RemoteDevices::NotLoaded,
    })
}

pub async fn disable() -> RemoteControlResult<RemoteControlSnapshot> {
    disable_inner().await.map_err(Into::into)
}

async fn disable_inner() -> Result<RemoteControlSnapshot> {
    commands::disable().await?;
    let environment_id = control_account_id()
        .as_ref()
        .map(store::environment)
        .transpose()?
        .flatten();
    Ok(RemoteControlSnapshot {
        environment_id,
        ..Default::default()
    })
}

pub async fn start_pairing() -> RemoteControlResult<RemotePairing> {
    start_pairing_inner().await.map_err(Into::into)
}

async fn start_pairing_inner() -> Result<RemotePairing> {
    let pairing = commands::pair().await?;
    if let Some(account) = control_account_id() {
        store::remember(&account, &pairing.environment_id)?;
    }
    Ok(pairing)
}

pub async fn pairing_claimed(pairing: &RemotePairing) -> RemoteControlResult<bool> {
    pairing_claimed_inner(pairing).await.map_err(Into::into)
}

async fn pairing_claimed_inner(pairing: &RemotePairing) -> Result<bool> {
    #[derive(Deserialize)]
    struct Response {
        claimed: bool,
    }
    let response: Response = rpc::request(
        &socket_path()?,
        "remoteControl/pairing/status",
        Some(json!({ "pairingCode": pairing.pairing_code() })),
    )
    .await?;
    Ok(response.claimed)
}

pub async fn revoke_device(environment_id: &str, client_id: &str) -> RemoteControlResult<()> {
    revoke_device_inner(environment_id, client_id)
        .await
        .map_err(Into::into)
}

async fn revoke_device_inner(environment_id: &str, client_id: &str) -> Result<()> {
    let _: serde_json::Value = rpc::request(
        &socket_path()?,
        "remoteControl/client/revoke",
        Some(json!({ "environmentId": environment_id, "clientId": client_id })),
    )
    .await?;
    Ok(())
}

pub async fn devices(environment: &str) -> RemoteControlResult<Vec<RemoteDevice>> {
    devices::list(
        &socket_path().map_err(RemoteControlFailure::from)?,
        environment,
    )
    .await
    .map_err(Into::into)
}

fn socket_path() -> Result<PathBuf> {
    let home = crate::limits::codex::codex_home().context("no Codex home directory")?;
    Ok(home
        .join("app-server-control")
        .join("app-server-control.sock"))
}

pub fn control_account_id() -> Option<crate::accounts::AccountId> {
    crate::accounts::discover_profiles()
        .into_iter()
        .find(|profile| profile.provider == crate::Provider::Codex && !profile.managed)
        .map(|profile| profile.account.id)
}

#[cfg(test)]
mod tests;
