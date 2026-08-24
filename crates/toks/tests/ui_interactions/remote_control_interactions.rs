use chrono::{TimeZone, Utc};
use gpui::{px, size, TestAppContext};
use toks::{
    test_support::{
        prepare_rotation_accounts, set_page, set_remote_control, set_rotation_service_active,
    },
    Page, ToksApp,
};
use toks_core::remote_control::{RemoteConnectionStatus, RemoteControlOwner};

use super::support::{remote_control_snapshot, Harness};

#[gpui::test]
fn remote_control_is_one_row_in_the_routing_card(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 23, 12, 0, 0)
        .single()
        .unwrap();
    let mut app = ToksApp::from_snapshots(None, vec![remote_control_snapshot(now)], now);
    prepare_rotation_accounts(&mut app);
    set_rotation_service_active(&mut app);
    set_remote_control(
        &mut app,
        RemoteConnectionStatus::Managed(RemoteControlOwner::ChatGptDesktop),
    );
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, size(px(1000.), px(800.)));

    let card = harness.bounds("rotation-status-card");
    let routing = harness.bounds("rotation-router-controls");
    let remote = harness.bounds("rotation-remote-control-row");
    assert!(card.contains(&routing.center()));
    assert!(card.contains(&remote.center()));
    assert!(remote.top() >= routing.bottom());
    assert!(remote.size.height <= px(48.));

    for selector in [
        "rotation-remote-control-status",
        "account-email-remote-control",
    ] {
        assert!(harness.has(selector), "missing {selector}");
    }
    for selector in [
        "rotation-remote-control-card",
        "rotation-remote-model-account",
        "rotation-remote-managed-in-chatgpt",
        "rotation-remote-devices-panel",
    ] {
        assert!(!harness.has(selector), "unexpected {selector}");
    }
}
