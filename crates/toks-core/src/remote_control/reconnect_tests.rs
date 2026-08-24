use super::reconnect::is_settled;
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
