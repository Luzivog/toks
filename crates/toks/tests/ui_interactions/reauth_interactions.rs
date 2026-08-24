#![cfg(feature = "test-support")]

use chrono::{TimeZone, Utc};
use gpui::{px, size, TestAppContext};
use toks::ToksApp;
use toks_core::limits::LimitIssueKind;
use toks_core::Provider;

use super::support::{failed_snapshot, Harness};

#[gpui::test]
fn only_authentication_issues_offer_exact_account_sign_in(cx: &mut TestAppContext) {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut limits = vec![
        failed_snapshot(
            Provider::Claude,
            "auth-claude",
            LimitIssueKind::Authentication,
            now,
        ),
        failed_snapshot(
            Provider::Codex,
            "auth-codex",
            LimitIssueKind::Authentication,
            now,
        ),
    ];
    limits.extend(
        [
            LimitIssueKind::Network,
            LimitIssueKind::RateLimited,
            LimitIssueKind::InvalidResponse,
            LimitIssueKind::Unavailable,
            LimitIssueKind::Storage,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, kind)| {
            failed_snapshot(Provider::Claude, &format!("other-{index}"), kind, now)
        }),
    );
    let app = ToksApp::from_snapshots(None, limits, now);
    let mut harness = Harness::open(cx, app, size(px(1400.), px(900.)));

    assert!(harness.has("reauthenticate-claude-auth-claude"));
    assert!(harness.has("reauthenticate-codex-auth-codex"));
    assert!(harness.has("quota-row-weekly-auth-claude"));
    assert!(harness.has("quota-row-weekly-auth-codex"));
    for selector in [
        "reauthenticate-claude-other-0",
        "reauthenticate-claude-other-1",
        "reauthenticate-claude-other-2",
        "reauthenticate-claude-other-3",
        "reauthenticate-claude-other-4",
    ] {
        assert!(!harness.has(selector));
    }
}
