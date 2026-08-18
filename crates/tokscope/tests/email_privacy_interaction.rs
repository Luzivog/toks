#![cfg(feature = "test-support")]

use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, Pixels, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use tokscope::test_support::{initialize, WindowFrame};
use tokscope::TokscopeApp;
use tokscope_core::limits::{SnapshotFreshness, SnapshotStatus};
use tokscope_core::{LimitSnapshot, Provider, ProviderAccount};

#[gpui::test]
fn email_privacy_overlay_preserves_account_header_layout(cx: &mut TestAppContext) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = cx.new(|_| {
        TokscopeApp::from_snapshots(
            None,
            vec![LimitSnapshot {
                provider: Provider::Codex,
                account: ProviderAccount {
                    id: "privacy".into(),
                    email: Some("hello@example.test".into()),
                },
                plan: Some("Pro".into()),
                plan_multiplier: None,
                windows: Vec::new(),
                extras: Vec::new(),
                fetched_at: Some(now),
                source: String::new(),
                issue: None,
                status: SnapshotStatus {
                    freshness: SnapshotFreshness::Live,
                    last_attempted_at: Some(now),
                    issue: None,
                },
            }],
            now,
        )
    });
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1200.), px(700.)),
                ))),
                window_background: WindowBackgroundAppearance::Opaque,
                window_decorations: Some(WindowDecorations::Client),
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            },
            |_, cx| cx.new(|_| WindowFrame::new(content)),
        )
        .expect("headless window opens")
    });
    let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
    cx.run_until_parked();

    let email_before = bounds(cx, "account-email-codex-privacy");
    let plan_before = bounds(cx, "account-plan-codex-privacy");
    let status_before = bounds(cx, "account-status-codex-privacy");
    assert!(cx
        .debug_bounds("account-email-blur-codex-privacy")
        .is_none());

    let toggle = bounds(cx, "toggle-account-emails").center();
    cx.simulate_mouse_move(toggle, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(toggle, Modifiers::none());
    cx.run_until_parked();

    assert!(cx
        .debug_bounds("account-email-blur-codex-privacy")
        .is_some());
    assert_eq!(email_before, bounds(cx, "account-email-codex-privacy"));
    assert_eq!(plan_before, bounds(cx, "account-plan-codex-privacy"));
    assert_eq!(status_before, bounds(cx, "account-status-codex-privacy"));
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
}
