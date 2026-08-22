use std::time::Duration;

use anyhow::{Context, Result};
use serde::Deserialize;
use tokio::{process::Command, time::timeout};

use super::{RemoteConnection, RemoteConnectionStatus, RemotePairing};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(100);

#[derive(Clone, Copy)]
enum Operation {
    Enable,
    Disable,
    Pair,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StartOutput {
    status: WireStatus,
    server_name: String,
    environment_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PairingOutput {
    pairing_code: String,
    manual_pairing_code: Option<String>,
    environment_id: String,
    expires_at: i64,
}

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "camelCase")]
enum WireStatus {
    Disabled,
    Connecting,
    Connected,
    Errored,
}

pub(super) async fn enable() -> Result<(RemoteConnection, Option<String>)> {
    let raw = run(arguments(Operation::Enable)).await?;
    parse_start(&raw)
}

pub(super) async fn disable() -> Result<()> {
    run(arguments(Operation::Disable)).await?;
    Ok(())
}

pub(super) async fn pair() -> Result<RemotePairing> {
    let raw = run(arguments(Operation::Pair)).await?;
    parse_pairing(&raw)
}

async fn run(args: &[&str]) -> Result<Vec<u8>> {
    let executable = crate::codex_router::codex_binary::discover()?;
    let mut command = Command::new(&executable);
    command.args(args).kill_on_drop(true);
    let output = timeout(COMMAND_TIMEOUT, command.output())
        .await
        .with_context(|| {
            format!(
                "Codex command timed out after {}s",
                COMMAND_TIMEOUT.as_secs()
            )
        })?
        .with_context(|| format!("running {} {}", executable.display(), args.join(" ")))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_string();
    anyhow::bail!(if message.is_empty() {
        "Codex command failed without an error message".into()
    } else {
        message
    })
}

fn arguments(operation: Operation) -> &'static [&'static str] {
    match operation {
        Operation::Enable => &["remote-control", "--json", "start"],
        Operation::Disable => &["app-server", "daemon", "disable-remote-control"],
        Operation::Pair => &["remote-control", "--json", "pair"],
    }
}

fn parse_start(raw: &[u8]) -> Result<(RemoteConnection, Option<String>)> {
    let output: StartOutput = serde_json::from_slice(raw).context("reading Codex Remote status")?;
    Ok((
        RemoteConnection {
            status: output.status.into(),
            server_name: Some(output.server_name),
        },
        output.environment_id,
    ))
}

fn parse_pairing(raw: &[u8]) -> Result<RemotePairing> {
    let output: PairingOutput =
        serde_json::from_slice(raw).context("reading Codex pairing response")?;
    Ok(RemotePairing {
        pairing_code: output.pairing_code,
        manual_code: output
            .manual_pairing_code
            .context("Codex did not return a manual pairing code")?,
        environment_id: output.environment_id,
        expires_at: output.expires_at,
    })
}

impl From<WireStatus> for RemoteConnectionStatus {
    fn from(value: WireStatus) -> Self {
        match value {
            WireStatus::Disabled => Self::Off,
            WireStatus::Connecting => Self::Connecting,
            WireStatus::Connected => Self::Connected,
            WireStatus::Errored => Self::Errored,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{arguments, parse_pairing, parse_start, Operation};
    use crate::remote_control::RemoteConnectionStatus;

    #[test]
    fn parses_machine_readable_lifecycle_and_pairing_output() {
        let (connection, environment) = parse_start(
            br#"{"mode":"daemon","status":"connected","serverName":"workstation","environmentId":"env","timedOut":false}"#,
        )
        .unwrap();
        assert_eq!(connection.status, RemoteConnectionStatus::Connected);
        assert_eq!(connection.server_name.as_deref(), Some("workstation"));
        assert_eq!(environment.as_deref(), Some("env"));

        let pairing = parse_pairing(
            br#"{"pairingCode":"opaque","manualPairingCode":"ABCD-EFGH","environmentId":"env","expiresAt":1777000000}"#,
        )
        .unwrap();
        assert_eq!(pairing.manual_code, "ABCD-EFGH");
        assert_eq!(pairing.environment_id, "env");
        assert_eq!(pairing.expires_at, 1_777_000_000);
    }

    #[test]
    fn lifecycle_commands_use_durable_machine_readable_operations() {
        assert_eq!(
            arguments(Operation::Enable),
            ["remote-control", "--json", "start"]
        );
        assert_eq!(
            arguments(Operation::Disable),
            ["app-server", "daemon", "disable-remote-control"]
        );
        assert_eq!(
            arguments(Operation::Pair),
            ["remote-control", "--json", "pair"]
        );
    }
}
