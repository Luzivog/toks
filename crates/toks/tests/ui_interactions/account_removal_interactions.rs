#![cfg(feature = "test-support")]

use chrono::{TimeZone, Utc};
use gpui::{px, size, TestAppContext};
use toks::ToksApp;

use super::support::{account_removal_snapshot, Harness};

#[gpui::test]
fn removal_actions_use_opaque_account_ids_and_confirm_before_removing(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = ToksApp::from_snapshots(
        None,
        vec![
            account_removal_snapshot("logical-first", "same@example.test", now),
            account_removal_snapshot("logical-second", "same@example.test", now),
        ],
        now,
    );
    let mut harness = Harness::open(cx, app, size(px(1400.), px(900.)));

    assert!(harness.has("account-actions-codex-logical-first"));
    assert!(harness.has("account-actions-codex-logical-second"));
    assert!(!harness.has("account-actions-codex-same@example.test"));

    harness.click("account-actions-codex-logical-first");
    assert!(harness.has("remove-account-codex-logical-first"));
    harness.click("remove-account-codex-logical-first");
    assert!(harness.has("account-removal-confirmation"));
    assert!(harness.has("account-removal-confirmation-copy"));

    // Opening the confirmation must not mutate the row before explicit consent.
    assert!(harness.has("account-group-codex-logical-first"));
    assert!(harness.has("account-group-codex-logical-second"));
}
