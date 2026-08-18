#![cfg(feature = "test-support")]

use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, TestAppContext, VisualTestContext,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use tokscope::test_support::{initialize, WindowFrame};
use tokscope::TokscopeApp;
use tokscope_core::limits::{
    LimitIssue, LimitIssueKind, LimitWindow, SnapshotFreshness, SnapshotStatus,
};
use tokscope_core::{LimitSnapshot, Provider, ProviderAccount};

#[gpui::test]
fn only_authentication_issues_offer_exact_account_sign_in(cx: &mut TestAppContext) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut limits = vec![failed_snapshot(
        "auth-account",
        LimitIssueKind::Authentication,
        now,
    )];
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
        .map(|(index, kind)| failed_snapshot(&format!("other-{index}"), kind, now)),
    );
    let app = cx.new(|_| TokscopeApp::from_snapshots(None, limits, now));
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1400.), px(900.)),
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

    assert!(has(cx, "reauthenticate-claude-auth-account"));
    assert!(has(cx, "quota-row-weekly-auth-account"));
    for selector in [
        "reauthenticate-claude-other-0",
        "reauthenticate-claude-other-1",
        "reauthenticate-claude-other-2",
        "reauthenticate-claude-other-3",
        "reauthenticate-claude-other-4",
    ] {
        assert!(!has(cx, selector));
    }
}

fn has(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn failed_snapshot(id: &str, kind: LimitIssueKind, now: chrono::DateTime<Utc>) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Claude,
        account: ProviderAccount {
            id: id.into(),
            email: Some(format!("{id}@example.test")),
        },
        plan: Some("Max".into()),
        plan_multiplier: None,
        windows: vec![LimitWindow {
            id: format!("weekly-{id}"),
            label: "Weekly".into(),
            percent_used: 42.0,
            resets_at: Some(now + chrono::Duration::days(6)),
            severity: None,
            scope: None,
            is_active: true,
            raw: Default::default(),
        }],
        extras: Vec::new(),
        fetched_at: Some(now - chrono::Duration::minutes(2)),
        source: "cache".into(),
        issue: None,
        status: SnapshotStatus {
            freshness: SnapshotFreshness::Cached,
            last_attempted_at: Some(now),
            issue: Some(LimitIssue {
                kind,
                message: "failed".into(),
                attempted_at: now,
                retry_at: None,
            }),
        },
    }
}
