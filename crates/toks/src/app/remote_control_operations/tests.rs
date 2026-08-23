use toks_core::remote_control::{
    RemoteConnection, RemoteConnectionStatus, RemoteControlFailure, RemoteControlFailureKind,
    RemoteControlSnapshot, RemoteDevice, RemoteDevices, RemotePairing,
};

use super::{RemoteControlUiState, RemotePanel};

#[test]
fn expired_pairing_returns_to_the_summary_without_touching_a_backend() {
    let mut state = RemoteControlUiState::default();
    state.panel = RemotePanel::Pairing;
    state.pairing = Some(RemotePairing::new(
        "opaque".into(),
        "ABCD-EFGH".into(),
        "environment".into(),
        100,
    ));
    state.expire_pairing(100);
    assert!(state.pairing.is_none());
    assert_eq!(state.panel, RemotePanel::Summary);
}

#[test]
fn revoke_requires_confirmation_and_cancel_is_local() {
    let mut state = RemoteControlUiState::default();
    state.confirm_revoke("phone".into());
    assert_eq!(state.pending_revoke.as_deref(), Some("phone"));
    state.cancel_revoke();
    assert!(state.pending_revoke.is_none());
}

#[test]
fn relay_updates_preserve_devices_and_do_not_overwrite_action_feedback() {
    let mut state = RemoteControlUiState::default();
    state.snapshot = snapshot(RemoteConnectionStatus::Errored);
    state.fail_action(failure(
        RemoteControlFailureKind::Other,
        "raw command stderr",
    ));
    state.fail_status(failure(
        RemoteControlFailureKind::DaemonUnavailable,
        "raw socket error",
    ));

    state.apply_snapshot(RemoteControlSnapshot {
        connection: RemoteConnection {
            status: RemoteConnectionStatus::Errored,
            server_name: Some("workstation".into()),
        },
        ..Default::default()
    });

    assert!(state.status_issue.is_none());
    assert_eq!(
        state.action_issue.as_ref().map(|issue| issue.kind),
        Some(RemoteControlFailureKind::Other)
    );
    assert_eq!(
        state.snapshot.environment_id.as_deref(),
        Some("environment")
    );
    assert!(matches!(state.snapshot.devices, RemoteDevices::Loaded(_)));

    state.apply_snapshot(snapshot(RemoteConnectionStatus::Connected));
    assert!(state.action_issue.is_none());
}

fn failure(kind: RemoteControlFailureKind, detail: &str) -> RemoteControlFailure {
    RemoteControlFailure {
        kind,
        detail: detail.into(),
    }
}

fn snapshot(status: RemoteConnectionStatus) -> RemoteControlSnapshot {
    RemoteControlSnapshot {
        connection: RemoteConnection {
            status,
            server_name: Some("workstation".into()),
        },
        environment_id: Some("environment".into()),
        devices: RemoteDevices::Loaded(vec![RemoteDevice {
            client_id: "phone".into(),
            display_name: None,
            device_type: None,
            platform: None,
            os_version: None,
            device_model: None,
            app_version: None,
            last_seen_at: None,
        }]),
    }
}
