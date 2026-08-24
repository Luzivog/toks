use super::remote_control::status_label;
use toks_core::remote_control::{RemoteConnectionStatus, RemoteControlOwner};

#[test]
fn desktop_ownership_has_compact_copy() {
    assert_eq!(
        status_label(RemoteConnectionStatus::Managed(
            RemoteControlOwner::ChatGptDesktop
        )),
        "On via ChatGPT"
    );
}
