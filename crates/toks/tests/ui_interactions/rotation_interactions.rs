use std::time::Duration;

use chrono::{TimeZone, Utc};
use gpui::{point, px, size, Modifiers, MouseButton, TestAppContext};
use toks::{
    test_support::{
        current_page, emails_hidden, prepare_rotation_accounts, set_page,
        set_rotation_active_threads, set_rotation_activity_unavailable, set_rotation_blocked,
        set_rotation_stale_transport_threads, set_rotation_thread_idle,
        set_rotation_thread_pending, set_rotation_thread_title, set_rotation_thread_waiting,
    },
    Page, ToksApp,
};

use super::support::{rotation_limit_snapshot, Harness, VIEWPORT};

#[gpui::test]
fn rotation_sidebar_entry_opens_the_private_dashboard(cx: &mut TestAppContext) {
    let app = ToksApp::from_snapshots(None, Vec::new(), Utc::now());
    let mut harness = Harness::open(cx, app, size(px(1400.), px(800.)));
    harness.click("rotation");

    assert_eq!(
        harness
            .app
            .read_with(harness.cx, |app, _| current_page(app)),
        Page::Rotation
    );
    assert!(harness.has("rotation-page"));
    let router_controls = harness.bounds("rotation-router-controls");
    assert!(router_controls.size.height <= px(48.));
    assert!(harness.has("rotation-routing-toggle"));
    assert!(!harness.has("rotation-fast-drain-toggle"));
    assert!(!harness.has("rotation-service-toggle"));

    let control = harness.bounds("rotation-routing-toggle");
    let label = point(control.right() - px(2.), control.center().y);
    harness
        .cx
        .simulate_mouse_move(label, None::<MouseButton>, Modifiers::none());
    harness.cx.executor().advance_clock(Duration::from_secs(1));
    harness.cx.run_until_parked();
    assert!(harness.has("rotation-routing-toggle-tooltip"));
}

#[gpui::test]
fn rotation_does_not_render_pending_threads_or_queue_controls(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app = ToksApp::from_snapshots(None, Vec::new(), now);
    set_rotation_thread_pending(&mut app, "queued-fixture");
    set_rotation_active_threads(&mut app, "unused", 0);
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, VIEWPORT);

    assert!(harness.has("rotation-active-threads-count"));
    assert!(!harness.has("rotation-thread-header-captions"));
    assert!(!harness.has("rotation-thread-captions"));
    let title = harness.bounds("rotation-active-threads-title");
    let count = harness.bounds("rotation-active-threads-count");
    assert_eq!(count.left() - title.right(), px(8.));
    assert_eq!(title.center().y, count.center().y);
    assert!(!harness.has("rotation-pending-threads-card"));
    assert!(!harness.has("rotation-thread-up-queued-fixture"));
    assert!(!harness.has("rotation-thread-down-queued-fixture"));
    assert!(!harness.has("rotation-cancel-thread-queued-fixture"));
}

#[gpui::test]
fn active_threads_render_binary_presence_and_aligned_request_selectors(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app = ToksApp::from_snapshots(
        None,
        vec![rotation_limit_snapshot(now, "active", 42.0)],
        now,
    );
    prepare_rotation_accounts(&mut app);
    set_rotation_active_threads(&mut app, "active", 3);
    set_rotation_thread_idle(&mut app, "active", "active-fixture-0");
    set_rotation_thread_waiting(&mut app, "active", "active-fixture-0");
    set_rotation_thread_waiting(&mut app, "active", "active-fixture-2");
    set_rotation_thread_idle(&mut app, "active", "idle-fixture");
    set_rotation_thread_title(&mut app, "active-fixture-0", "Repair router handoff");
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, VIEWPORT);

    assert!(harness.has("rotation-thread-row-active-fixture-0"));
    assert!(harness.has("rotation-thread-row-active-fixture-1"));
    assert!(harness.has("rotation-thread-row-active-fixture-2"));
    assert!(!harness.has("rotation-thread-row-idle-fixture"));
    assert!(harness.has("rotation-thread-title-active-fixture-0"));
    assert!(harness.has("rotation-active-threads-count"));
    assert!(harness.has("rotation-thread-header-captions"));
    assert!(!harness.has("rotation-thread-captions"));
    assert!(harness.has("rotation-thread-model-active-fixture-0"));
    assert!(harness.has("rotation-thread-reasoning-active-fixture-0"));
    assert!(harness.has("rotation-thread-tier-active-fixture-0"));
    assert!(!harness.has("rotation-thread-status-active-fixture-0"));
    assert!(!harness.has("rotation-thread-status-dot-active-fixture-0"));
    assert!(!harness.has("rotation-dismiss-thread-active-fixture-0"));
    assert!(!harness.has("rotation-dismiss-thread-active-fixture-1"));

    let title = harness.bounds("rotation-active-threads-title");
    let count = harness.bounds("rotation-active-threads-count");
    let caption_label = harness.bounds("rotation-thread-caption-model-label");
    assert_eq!(count.left() - title.right(), px(8.));
    assert_eq!(title.center().y, count.center().y);
    assert!((count.center().y - caption_label.center().y).abs() <= px(1.));

    for (caption, caption_label, first, first_value, second, width) in [
        (
            "rotation-thread-caption-model",
            "rotation-thread-caption-model-label",
            "rotation-thread-model-active-fixture-0",
            "rotation-thread-model-active-fixture-0-value",
            "rotation-thread-model-active-fixture-1",
            140.,
        ),
        (
            "rotation-thread-caption-reasoning",
            "rotation-thread-caption-reasoning-label",
            "rotation-thread-reasoning-active-fixture-0",
            "rotation-thread-reasoning-active-fixture-0-value",
            "rotation-thread-reasoning-active-fixture-1",
            80.,
        ),
        (
            "rotation-thread-caption-tier",
            "rotation-thread-caption-tier-label",
            "rotation-thread-tier-active-fixture-0",
            "rotation-thread-tier-active-fixture-0-value",
            "rotation-thread-tier-active-fixture-1",
            80.,
        ),
    ] {
        let caption = harness.bounds(caption);
        let caption_label = harness.bounds(caption_label);
        let first = harness.bounds(first);
        let first_value = harness.bounds(first_value);
        let second = harness.bounds(second);
        assert_eq!(caption.left(), first.left());
        assert_eq!(caption.size.width, first.size.width);
        assert_eq!(first.left(), second.left());
        assert_eq!(first.size.width, second.size.width);
        assert_eq!(first.size.width, px(width));
        assert!((caption_label.right() - first_value.right()).abs() <= px(1.));
    }
}

#[gpui::test]
fn active_threads_ignore_twenty_five_stale_transport_claims(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app = ToksApp::from_snapshots(
        None,
        vec![rotation_limit_snapshot(now, "active", 42.0)],
        now,
    );
    prepare_rotation_accounts(&mut app);
    set_rotation_active_threads(&mut app, "active", 1);
    set_rotation_stale_transport_threads(&mut app, "active", 24);
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, VIEWPORT);

    assert!(harness.has("rotation-active-threads-count-1"));
    assert!(harness.has("rotation-thread-row-active-fixture-0"));
    assert!(!harness.has("rotation-thread-row-stale-transport-fixture-0"));
    assert!(!harness.has("rotation-thread-row-stale-transport-fixture-23"));
}

#[gpui::test]
fn unavailable_activity_does_not_render_a_numeric_count(cx: &mut TestAppContext) {
    let mut app = ToksApp::from_snapshots(None, Vec::new(), Utc::now());
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, VIEWPORT);

    assert!(harness.has("rotation-active-threads-unavailable"));
    assert!(!harness.has("rotation-active-threads-count"));
}

#[gpui::test]
fn unavailable_refresh_clears_previously_visible_activity(cx: &mut TestAppContext) {
    let mut app = ToksApp::from_snapshots(None, Vec::new(), Utc::now());
    set_rotation_active_threads(&mut app, "active", 1);
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, VIEWPORT);
    assert!(harness.has("rotation-active-threads-count-1"));
    assert!(harness.has("rotation-thread-row-active-fixture-0"));

    harness.app.update(harness.cx, |app, cx| {
        set_rotation_activity_unavailable(app);
        cx.notify();
    });
    harness.cx.run_until_parked();

    assert!(harness.has("rotation-active-threads-unavailable"));
    assert!(!harness.has("rotation-active-threads-count"));
    assert!(!harness.has("rotation-thread-row-active-fixture-0"));
}

#[gpui::test]
fn active_threads_do_not_silently_omit_rows(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app = ToksApp::from_snapshots(
        None,
        vec![rotation_limit_snapshot(now, "active", 42.0)],
        now,
    );
    prepare_rotation_accounts(&mut app);
    set_rotation_active_threads(&mut app, "active", 101);
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, VIEWPORT);

    assert!(harness.has("rotation-thread-row-active-fixture-99"));
}

#[gpui::test]
fn rotation_account_quota_is_compact_and_exact_time_moves_to_a_tooltip(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app =
        ToksApp::from_snapshots(None, vec![rotation_limit_snapshot(now, "quiet", 36.0)], now);
    prepare_rotation_accounts(&mut app);
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, size(px(1400.), px(800.)));

    let status = harness.bounds("rotation-account-status-quiet");
    let meter = harness.bounds("rotation-account-meter-quiet");
    assert!(meter.size.width <= px(72.));
    assert!(meter.size.height <= px(3.));
    assert!((status.center().y - meter.center().y).abs() <= px(4.));

    harness
        .cx
        .simulate_mouse_move(status.center(), None::<MouseButton>, Modifiers::none());
    harness.cx.executor().advance_clock(Duration::from_secs(1));
    harness.cx.run_until_parked();
    assert!(harness.has("rotation-account-status-tooltip-quiet"));
}

#[gpui::test]
fn active_thread_counts_do_not_shift_account_meters(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app = ToksApp::from_snapshots(
        None,
        vec![
            rotation_limit_snapshot(now, "idle", 100.0),
            rotation_limit_snapshot(now, "active", 42.0),
        ],
        now,
    );
    prepare_rotation_accounts(&mut app);
    set_rotation_active_threads(&mut app, "active", 7);
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, size(px(1400.), px(800.)));

    let idle = harness.bounds("rotation-account-meter-idle");
    let active = harness.bounds("rotation-account-meter-active");
    assert_eq!(idle.left(), active.left());
    assert_eq!(idle.right(), active.right());
}

#[gpui::test]
fn rotation_hides_emails_and_confirms_resets_without_spending_one(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 22, 19, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut snapshot = rotation_limit_snapshot(now, "resettable", 100.0);
    snapshot.banked_resets = 1;
    let mut app = ToksApp::from_snapshots(None, vec![snapshot], now);
    prepare_rotation_accounts(&mut app);
    set_rotation_active_threads(&mut app, "resettable", 1);
    set_rotation_blocked(&mut app, "resettable");
    set_page(&mut app, Page::Rotation);
    let mut harness = Harness::open(cx, app, size(px(1400.), px(800.)));

    assert!(!harness.has("rotation-use-now-resettable"));
    let privacy = harness.bounds("rotation-toggle-account-emails").center();
    harness
        .cx
        .simulate_mouse_move(privacy, None::<MouseButton>, Modifiers::none());
    harness.cx.simulate_click(privacy, Modifiers::none());
    harness.cx.run_until_parked();
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| emails_hidden(app)));

    let use_reset = harness.bounds("rotation-use-reset-resettable").center();
    harness
        .cx
        .simulate_mouse_move(use_reset, None::<MouseButton>, Modifiers::none());
    harness.cx.simulate_click(use_reset, Modifiers::none());
    harness.cx.run_until_parked();
    assert!(harness.has("rotation-confirm-reset-resettable"));
    assert!(!harness.has("rotation-reset-error"));
    assert!(!harness.has("rotation-dismiss-reset-notice"));

    harness.click("rotation-cancel-reset-resettable");

    assert!(harness.has("rotation-use-reset-resettable"));
}
