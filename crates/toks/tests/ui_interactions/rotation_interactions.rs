use std::{ops::Deref, time::Duration};

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, TestAppContext, VisualTestContext,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::{
    test_support::{
        current_page, emails_hidden, initialize, prepare_rotation_accounts, set_page,
        set_rotation_active_threads, set_rotation_blocked, WindowFrame,
    },
    Page, ToksApp,
};
use toks_core::accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind};
use toks_core::limits::{LimitWindow, SnapshotFreshness, SnapshotStatus};
use toks_core::{LimitSnapshot, Provider, ProviderAccount};

#[gpui::test]
fn rotation_sidebar_entry_opens_the_private_dashboard(cx: &mut TestAppContext) {
    initialize(cx);
    let app = cx.new(|_| ToksApp::from_snapshots(None, Vec::new(), Utc::now()));
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1400.), px(800.)),
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

    let rotation = cx
        .debug_bounds("rotation")
        .expect("rotation sidebar entry is rendered")
        .center();
    cx.simulate_mouse_move(rotation, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(rotation, Modifiers::none());
    cx.run_until_parked();

    assert_eq!(
        app.read_with(cx, |app, _| current_page(app)),
        Page::Rotation
    );
    assert!(cx.debug_bounds("rotation-page").is_some());
    let router_controls = cx
        .debug_bounds("rotation-router-controls")
        .expect("router controls are rendered");
    assert!(router_controls.size.height <= px(48.));
    for selector in ["rotation-routing-toggle", "rotation-fast-drain-toggle"] {
        assert!(
            cx.debug_bounds(selector).is_some(),
            "{selector} should render as a switch"
        );
    }
    assert!(cx.debug_bounds("rotation-service-toggle").is_none());

    for (selector, tooltip_selector) in [
        ("rotation-routing-toggle", "rotation-routing-toggle-tooltip"),
        (
            "rotation-fast-drain-toggle",
            "rotation-fast-drain-toggle-tooltip",
        ),
    ] {
        let control = cx.debug_bounds(selector).expect("control is rendered");
        let label = point(control.right() - px(2.), control.center().y);
        cx.simulate_mouse_move(label, None::<MouseButton>, Modifiers::none());
        cx.executor().advance_clock(Duration::from_secs(1));
        cx.run_until_parked();
        assert!(
            cx.debug_bounds(tooltip_selector).is_some(),
            "{selector} label should show its tooltip"
        );
    }
}

#[gpui::test]
fn rotation_account_quota_is_compact_and_exact_time_moves_to_a_tooltip(cx: &mut TestAppContext) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(None, vec![limit_snapshot(now, "quiet", 36.0)], now);
        prepare_rotation_accounts(&mut app);
        set_page(&mut app, Page::Rotation);
        app
    });
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1400.), px(800.)),
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

    let status = cx
        .debug_bounds("rotation-account-status-quiet")
        .expect("account status is rendered");
    let meter = cx
        .debug_bounds("rotation-account-meter-quiet")
        .expect("weekly meter is rendered");
    assert!(meter.size.width <= px(72.));
    assert!(meter.size.height <= px(3.));
    assert!((status.center().y - meter.center().y).abs() <= px(4.));

    cx.simulate_mouse_move(status.center(), None::<MouseButton>, Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(cx
        .debug_bounds("rotation-account-status-tooltip-quiet")
        .is_some());
}

#[gpui::test]
fn active_thread_counts_do_not_shift_account_meters(cx: &mut TestAppContext) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = cx.new(|_| {
        let mut app = ToksApp::from_snapshots(
            None,
            vec![
                limit_snapshot(now, "idle", 100.0),
                limit_snapshot(now, "active", 42.0),
            ],
            now,
        );
        prepare_rotation_accounts(&mut app);
        set_rotation_active_threads(&mut app, "active", 7);
        set_page(&mut app, Page::Rotation);
        app
    });
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1400.), px(800.)),
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

    let idle = cx
        .debug_bounds("rotation-account-meter-idle")
        .expect("idle account meter is rendered");
    let active = cx
        .debug_bounds("rotation-account-meter-active")
        .expect("active account meter is rendered");
    assert_eq!(idle.left(), active.left());
    assert_eq!(idle.right(), active.right());
}

#[gpui::test]
fn rotation_hides_emails_and_confirms_resets_without_spending_one(cx: &mut TestAppContext) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = cx.new(|_| {
        let mut snapshot = limit_snapshot(now, "resettable", 100.0);
        snapshot.banked_resets = 1;
        let mut app = ToksApp::from_snapshots(None, vec![snapshot], now);
        prepare_rotation_accounts(&mut app);
        set_rotation_active_threads(&mut app, "resettable", 1);
        set_rotation_blocked(&mut app, "resettable");
        set_page(&mut app, Page::Rotation);
        app
    });
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1400.), px(800.)),
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

    assert!(cx.debug_bounds("rotation-use-now-resettable").is_none());
    let privacy = cx
        .debug_bounds("rotation-toggle-account-emails")
        .expect("rotation email privacy action renders")
        .center();
    cx.simulate_mouse_move(privacy, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(privacy, Modifiers::none());
    cx.run_until_parked();
    assert!(app.read_with(cx, |app, _| emails_hidden(app)));

    let use_reset = cx
        .debug_bounds("rotation-use-reset-resettable")
        .expect("banked reset action renders only for the blocked account")
        .center();
    cx.simulate_mouse_move(use_reset, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(use_reset, Modifiers::none());
    cx.run_until_parked();
    assert!(cx
        .debug_bounds("rotation-confirm-reset-resettable")
        .is_some());
}

fn limit_snapshot(now: chrono::DateTime<Utc>, id: &str, percent_used: f64) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some(format!("{id}@example.test")),
            sources: vec![AccountSource {
                profile_id: format!("{id}-profile").into(),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
        plan: None,
        plan_multiplier: None,
        banked_resets: 0,
        banked_reset_credits: None,
        windows: vec![LimitWindow {
            id: format!("weekly-{id}"),
            label: "Weekly".into(),
            percent_used,
            resets_at: Some(now + chrono::Duration::days(6)),
            severity: None,
            scope: None,
            is_active: true,
            raw: Default::default(),
        }],
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
