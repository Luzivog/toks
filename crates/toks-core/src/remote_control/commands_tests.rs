use super::commands::{arguments, parse_pairing, parse_start, Operation};
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
        arguments(Operation::Reconnect),
        ["app-server", "daemon", "restart"]
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
