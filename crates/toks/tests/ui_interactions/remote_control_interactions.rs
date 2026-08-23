use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, Pixels, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::{
    test_support::{
        emails_hidden, exclude_rotation_account, initialize, prepare_rotation_accounts, set_page,
        set_remote_control, set_rotation_active_threads, set_rotation_service_active,
        show_remote_devices, show_remote_pairing, WindowFrame,
    },
    Page, ToksApp,
};
use toks_core::{
    accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind},
    limits::{SnapshotFreshness, SnapshotStatus},
    remote_control::{RemoteConnectionStatus, RemoteDevice},
    LimitSnapshot, Provider, ProviderAccount,
};

#[gpui::test]
fn remote_control_keeps_connection_identity_separate_and_private(cx: &mut TestAppContext) {
    let now = fixture_time();
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(
            None,
            vec![account(now, "control", true), account(now, "worker", false)],
            now,
        );
        prepare_rotation_accounts(&mut app);
        set_rotation_service_active(&mut app);
        exclude_rotation_account(&mut app, "control");
        set_rotation_active_threads(&mut app, "worker", 1);
        set_remote_control(&mut app, RemoteConnectionStatus::Connected, vec![phone()]);
        show_remote_devices(&mut app);
        set_page(&mut app, Page::Rotation);
        app
    });
    let cx = harness(cx, &app, 1400.);

    for selector in [
        "rotation-remote-control-card",
        "rotation-remote-status",
        "rotation-remote-control-account",
        "rotation-remote-model-account",
        "rotation-remote-add-device",
        "rotation-remote-manage-devices",
        "rotation-remote-device-phone",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
    let connection_before = bounds(cx, "account-email-remote-control");
    click(cx, "rotation-toggle-account-emails");
    assert!(app.read_with(cx, |app, _| emails_hidden(app)));
    assert_eq!(
        connection_before,
        bounds(cx, "account-email-remote-control")
    );
    let connection_blur = bounds(cx, "account-email-blur-remote-control");
    assert_eq!(connection_before, connection_blur);
    assert!(
        connection_blur.size.width < px(220.),
        "connection email haze must not fill the value column"
    );
    for selector in [
        "account-email-blur-rotation-priority-control",
        "account-email-blur-rotation-priority-worker",
        "account-email-blur-remote-model-worker",
        "account-email-blur-rotation-selected-worker",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
    assert!(cx
        .debug_bounds("account-email-blur-rotation-event-1787486400000-worker")
        .is_some());
    let control_hidden = bounds(cx, "account-email-rotation-priority-control");
    let worker_hidden = bounds(cx, "account-email-rotation-priority-worker");
    let model_hidden = bounds(cx, "account-email-remote-model-worker");
    let selected_hidden = bounds(cx, "account-email-rotation-selected-worker");
    let event_hidden = bounds(cx, "account-email-rotation-event-1787486400000-worker");
    click(cx, "rotation-toggle-account-emails");
    assert_eq!(
        control_hidden,
        bounds(cx, "account-email-rotation-priority-control")
    );
    assert_eq!(
        worker_hidden,
        bounds(cx, "account-email-rotation-priority-worker")
    );
    assert_eq!(
        model_hidden,
        bounds(cx, "account-email-remote-model-worker")
    );
    assert_eq!(
        selected_hidden,
        bounds(cx, "account-email-rotation-selected-worker")
    );
    assert_eq!(
        event_hidden,
        bounds(cx, "account-email-rotation-event-1787486400000-worker")
    );

    click(cx, "rotation-remote-remove-phone");
    assert!(cx
        .debug_bounds("rotation-remote-confirm-remove-phone")
        .is_some());
    click(cx, "rotation-remote-cancel-remove-phone");
    assert!(cx.debug_bounds("rotation-remote-remove-phone").is_some());
}

#[gpui::test]
fn paired_device_text_stays_inside_its_row(cx: &mut TestAppContext) {
    let now = fixture_time();
    let app = cx.new(|_| {
        let mut device = phone();
        device.display_name = Some("iOS 26.6 iPhone".into());
        let mut app = ToksApp::from_snapshots(
            None,
            vec![account(now, "control", true), account(now, "worker", false)],
            now,
        );
        prepare_rotation_accounts(&mut app);
        set_remote_control(&mut app, RemoteConnectionStatus::Errored, vec![device]);
        show_remote_devices(&mut app);
        set_page(&mut app, Page::Rotation);
        app
    });
    let cx = harness(cx, &app, 1000.);

    let row = bounds(cx, "rotation-remote-device-phone");
    for selector in [
        "rotation-remote-device-phone-title",
        "rotation-remote-device-phone-detail",
        "rotation-remote-remove-phone",
    ] {
        let child = bounds(cx, selector);
        assert!(
            row.contains(&child.center()),
            "{selector} must stay inside its row: row={row:?}, child={child:?}"
        );
    }
}

#[gpui::test]
fn errored_remote_control_offers_reconnect_instead_of_start(cx: &mut TestAppContext) {
    let now = fixture_time();
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(None, vec![account(now, "control", true)], now);
        prepare_rotation_accounts(&mut app);
        set_remote_control(&mut app, RemoteConnectionStatus::Errored, vec![phone()]);
        set_page(&mut app, Page::Rotation);
        app
    });
    let cx = harness(cx, &app, 1000.);

    assert!(cx.debug_bounds("rotation-remote-reconnect").is_some());
    assert!(cx.debug_bounds("rotation-remote-retry").is_none());
}

#[gpui::test]
fn pairing_code_is_inline_and_expiring(cx: &mut TestAppContext) {
    let now = fixture_time();
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(None, vec![account(now, "control", true)], now);
        prepare_rotation_accounts(&mut app);
        set_remote_control(&mut app, RemoteConnectionStatus::Connected, Vec::new());
        show_remote_pairing(&mut app, now.timestamp());
        set_page(&mut app, Page::Rotation);
        app
    });
    let cx = harness(cx, &app, 1400.);
    for selector in [
        "rotation-remote-pairing-panel",
        "rotation-remote-pairing-code",
        "rotation-remote-pairing-expires",
        "rotation-remote-copy-code",
        "rotation-remote-cancel-pairing",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
}

fn harness(
    cx: &mut TestAppContext,
    app: &gpui::Entity<ToksApp>,
    width: f32,
) -> &'static mut VisualTestContext {
    initialize(cx);
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(width), px(1000.)),
                ))),
                window_background: WindowBackgroundAppearance::Opaque,
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            },
            |_, cx| cx.new(|_| WindowFrame::new(content)),
        )
        .unwrap()
    });
    let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
    cx.run_until_parked();
    cx
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let point = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_move(point, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(point, Modifiers::none());
    cx.run_until_parked();
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
}

fn fixture_time() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 23, 12, 0, 0)
        .single()
        .unwrap()
}

fn account(now: chrono::DateTime<Utc>, id: &str, current: bool) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some(format!("{id}@example.test")),
            sources: vec![AccountSource {
                profile_id: format!("{id}-profile").into(),
                kind: if current {
                    CredentialProfileKind::Current
                } else {
                    CredentialProfileKind::Managed
                },
                primary: true,
            }],
        },
        plan: None,
        plan_multiplier: None,
        banked_resets: 0,
        banked_reset_credits: None,
        windows: Vec::new(),
        extras: Vec::new(),
        fetched_at: Some(now),
        source: "fixture".into(),
        issue: None,
        status: SnapshotStatus {
            freshness: SnapshotFreshness::Live,
            last_attempted_at: Some(now),
            issue: None,
        },
    }
}

fn phone() -> RemoteDevice {
    RemoteDevice {
        client_id: "phone".into(),
        display_name: Some("Thomas's phone".into()),
        device_type: Some("phone".into()),
        platform: Some("iOS".into()),
        os_version: Some("19".into()),
        device_model: None,
        app_version: Some("1.0".into()),
        last_seen_at: Some(1_777_118_340),
    }
}
