#![cfg(feature = "test-support")]

use chrono::{TimeZone, Utc};
use gpui::{px, size, Modifiers, MouseButton, TestAppContext};
use toks::ToksApp;

use super::support::{privacy_snapshot, Harness};

#[gpui::test]
fn email_privacy_overlay_preserves_account_header_layout(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = ToksApp::from_snapshots(None, vec![privacy_snapshot(now)], now);
    let mut harness = Harness::open(cx, app, size(px(1200.), px(1800.)));

    let email_before = harness.bounds("account-email-codex-privacy");
    let header_before = harness.bounds("account-header-codex-privacy");
    let plan_before = harness.bounds("account-plan-codex-privacy");
    let status_before = harness.bounds("account-status-codex-privacy");
    assert!(!harness.has("account-email-blur-codex-privacy"));

    let toggle = harness.bounds("toggle-account-emails").center();
    harness
        .cx
        .simulate_mouse_move(toggle, None::<MouseButton>, Modifiers::none());
    harness.cx.simulate_click(toggle, Modifiers::none());
    harness.cx.run_until_parked();

    let email_after = harness.bounds("account-email-codex-privacy");
    let blur = harness.bounds("account-email-blur-codex-privacy");
    assert_eq!(email_before, email_after);
    assert_eq!(email_after, blur, "privacy haze must stay inside the email");
    assert_eq!(
        header_before,
        harness.bounds("account-header-codex-privacy")
    );
    assert_eq!(plan_before, harness.bounds("account-plan-codex-privacy"));
    assert_eq!(
        status_before,
        harness.bounds("account-status-codex-privacy")
    );
}
