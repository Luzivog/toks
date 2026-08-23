use std::time::Duration;

use anyhow::{bail, Result};

use super::{RemoteConnectionStatus, RemoteControlSnapshot};

const MAX_STATUS_READS: usize = 30;
const POLL_INTERVAL: Duration = Duration::from_secs(1);

pub(super) async fn run() -> Result<RemoteControlSnapshot> {
    let current = super::status_inner().await?;
    if matches!(
        current.connection.status,
        RemoteConnectionStatus::Managed(_)
    ) {
        return Ok(current);
    }
    super::commands::reconnect().await?;
    for attempt in 0..MAX_STATUS_READS {
        if attempt > 0 {
            pause().await?;
        }
        match super::status_inner().await {
            Ok(snapshot) if is_settled(&snapshot) => return Ok(snapshot),
            Ok(_) | Err(_) => {}
        }
    }
    bail!("Timed out waiting for the Remote Control relay after restarting Codex")
}

fn is_settled(snapshot: &RemoteControlSnapshot) -> bool {
    matches!(
        snapshot.connection.status,
        RemoteConnectionStatus::Connected | RemoteConnectionStatus::Managed(_)
    )
}

async fn pause() -> Result<()> {
    super::runtime::run(async move {
        tokio::time::sleep(POLL_INTERVAL).await;
        Ok(())
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::is_settled;
    use crate::remote_control::{
        RemoteConnection, RemoteConnectionStatus, RemoteControlOwner, RemoteControlSnapshot,
    };

    #[test]
    fn only_terminal_relay_states_finish_reconnection() {
        for (status, expected) in [
            (RemoteConnectionStatus::Off, false),
            (RemoteConnectionStatus::Connecting, false),
            (RemoteConnectionStatus::Connected, true),
            (
                RemoteConnectionStatus::Managed(RemoteControlOwner::ChatGptDesktop),
                true,
            ),
            (RemoteConnectionStatus::Errored, false),
        ] {
            let snapshot = RemoteControlSnapshot {
                connection: RemoteConnection {
                    status,
                    server_name: None,
                },
                ..Default::default()
            };
            assert_eq!(is_settled(&snapshot), expected);
        }
    }
}
