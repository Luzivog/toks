#![cfg(feature = "test-support")]

use std::time::Duration;

use chrono::{TimeZone, Utc};
use gpui::{px, size, Modifiers, MouseButton, TestAppContext};
use toks::ToksApp;
use toks_core::Provider;

use super::support::{banked_reset_snapshot, Harness};

#[gpui::test]
fn banked_resets_follow_the_plan_and_render_for_positive_codex_counts_only(
    cx: &mut TestAppContext,
) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let limits = vec![
        banked_reset_snapshot(Provider::Codex, "positive", 2, now),
        banked_reset_snapshot(Provider::Codex, "unavailable", 1, now),
        banked_reset_snapshot(Provider::Codex, "zero", 0, now),
        banked_reset_snapshot(Provider::Claude, "claude", 2, now),
    ];
    let app = ToksApp::from_snapshots(None, limits, now);
    let mut harness = Harness::open(cx, app, size(px(1200.), px(1800.)));

    let plan = harness.bounds("account-plan-codex-positive");
    let resets = harness.bounds("account-resets-codex-positive");
    assert!(plan.right() < resets.left());
    assert!((plan.center().y - resets.center().y).abs() <= px(1.));
    assert!(resets.right() < harness.bounds("account-status-codex-positive").left());
    assert!(!harness.has("account-resets-codex-zero"));
    assert!(!harness.has("account-resets-claude-claude"));

    harness
        .cx
        .simulate_mouse_move(resets.center(), None::<MouseButton>, Modifiers::none());
    harness.cx.executor().advance_clock(Duration::from_secs(1));
    harness.cx.run_until_parked();
    assert!(harness.has("banked-reset-tooltip"));
    assert!(harness.has("banked-reset-credit-0"));
    assert!(harness.has("banked-reset-credit-1"));
    assert!(!harness.has("banked-reset-credit-2"));
    assert!(!harness.has("banked-reset-details-unavailable"));

    let reset = harness.bounds("quota-reset-weekly-positive");
    harness
        .cx
        .simulate_mouse_move(reset.center(), None::<MouseButton>, Modifiers::none());
    harness.cx.executor().advance_clock(Duration::from_secs(1));
    harness.cx.run_until_parked();
    assert!(harness.has("quota-reset-tooltip-weekly-positive"));

    let unavailable = harness.bounds("account-resets-codex-unavailable");
    harness
        .cx
        .simulate_mouse_move(unavailable.center(), None::<MouseButton>, Modifiers::none());
    harness.cx.executor().advance_clock(Duration::from_secs(1));
    harness.cx.run_until_parked();
    assert!(harness.has("banked-reset-details-unavailable"));
}
