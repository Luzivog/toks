use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Pixels, TestAppContext, VisualTestContext,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::{
    test_support::{
        initialize, prepare_rotation_accounts, set_page, set_remote_control,
        set_rotation_service_active, WindowFrame,
    },
    Page, ToksApp,
};
use toks_core::{
    accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind},
    limits::{SnapshotFreshness, SnapshotStatus},
    remote_control::{RemoteConnectionStatus, RemoteControlOwner},
    LimitSnapshot, Provider, ProviderAccount,
};

#[gpui::test]
fn remote_control_is_one_row_in_the_routing_card(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 23, 12, 0, 0)
        .single()
        .unwrap();
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(None, vec![account(now)], now);
        prepare_rotation_accounts(&mut app);
        set_rotation_service_active(&mut app);
        set_remote_control(
            &mut app,
            RemoteConnectionStatus::Managed(RemoteControlOwner::ChatGptDesktop),
        );
        set_page(&mut app, Page::Rotation);
        app
    });
    let cx = harness(cx, &app);

    let card = bounds(cx, "rotation-status-card");
    let routing = bounds(cx, "rotation-router-controls");
    let remote = bounds(cx, "rotation-remote-control-row");
    assert!(card.contains(&routing.center()));
    assert!(card.contains(&remote.center()));
    assert!(remote.top() >= routing.bottom());
    assert!(remote.size.height <= px(48.));

    for selector in [
        "rotation-remote-control-status",
        "account-email-remote-control",
    ] {
        assert!(cx.debug_bounds(selector).is_some(), "missing {selector}");
    }
    for selector in [
        "rotation-remote-control-card",
        "rotation-remote-model-account",
        "rotation-remote-managed-in-chatgpt",
        "rotation-remote-devices-panel",
    ] {
        assert!(cx.debug_bounds(selector).is_none(), "unexpected {selector}");
    }
}

fn harness(cx: &mut TestAppContext, app: &gpui::Entity<ToksApp>) -> &'static mut VisualTestContext {
    initialize(cx);
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1000.), px(800.)),
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

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
}

fn account(now: chrono::DateTime<Utc>) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: "control".into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some("hello@example.test".into()),
            sources: vec![AccountSource {
                profile_id: "control-profile".into(),
                kind: CredentialProfileKind::Current,
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
