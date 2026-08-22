use toks_core::remote_control::RemotePairing;

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
