#![cfg(feature = "test-support")]

use std::{ops::Deref, time::Duration};

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, Pixels, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::test_support::{initialize, WindowFrame};
use toks::ToksApp;
use toks_core::{
    accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind},
    limits::{
        BankedResetCredit, BankedResetCreditStatus, PlanMultiplier, SnapshotFreshness,
        SnapshotStatus,
    },
    LimitSnapshot, LimitWindow, Provider, ProviderAccount,
};

#[gpui::test]
fn banked_resets_follow_the_plan_and_render_for_positive_codex_counts_only(
    cx: &mut TestAppContext,
) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 21, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let limits = vec![
        snapshot(Provider::Codex, "positive", 2, now),
        snapshot(Provider::Codex, "unavailable", 1, now),
        snapshot(Provider::Codex, "zero", 0, now),
        snapshot(Provider::Claude, "claude", 2, now),
    ];
    let app = cx.new(|_| ToksApp::from_snapshots(None, limits, now));
    let content = app.clone();
    let window = cx.update(|cx| {
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(1200.), px(1800.)),
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

    let plan = bounds(cx, "account-plan-codex-positive");
    let resets = bounds(cx, "account-resets-codex-positive");
    assert!(plan.right() < resets.left());
    assert!((plan.center().y - resets.center().y).abs() <= px(1.));
    assert!(resets.right() < bounds(cx, "account-status-codex-positive").left());
    assert!(!has(cx, "account-resets-codex-zero"));
    assert!(!has(cx, "account-resets-claude-claude"));

    cx.simulate_mouse_move(resets.center(), None::<MouseButton>, Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(has(cx, "banked-reset-tooltip"));
    assert!(has(cx, "banked-reset-credit-0"));
    assert!(has(cx, "banked-reset-credit-1"));
    assert!(!has(cx, "banked-reset-credit-2"));
    assert!(!has(cx, "banked-reset-details-unavailable"));

    let reset = bounds(cx, "quota-reset-weekly-positive");
    cx.simulate_mouse_move(reset.center(), None::<MouseButton>, Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(has(cx, "quota-reset-tooltip-weekly-positive"));

    let unavailable = bounds(cx, "account-resets-codex-unavailable");
    cx.simulate_mouse_move(unavailable.center(), None::<MouseButton>, Modifiers::none());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    assert!(has(cx, "banked-reset-details-unavailable"));
}

fn snapshot(
    provider: Provider,
    id: &str,
    banked_resets: u64,
    now: chrono::DateTime<Utc>,
) -> LimitSnapshot {
    LimitSnapshot {
        provider,
        account: ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: None,
            sources: vec![AccountSource {
                profile_id: format!("profile-{id}").into(),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
        plan: Some("pro".into()),
        plan_multiplier: Some(PlanMultiplier::Twenty),
        banked_resets,
        banked_reset_credits: (provider == Provider::Codex && id == "positive").then(|| {
            vec![
                BankedResetCredit {
                    expires_at: Some(now + chrono::Duration::hours(1)),
                    title: Some("Redeemed reset".into()),
                    status: Some(BankedResetCreditStatus::Redeemed),
                },
                BankedResetCredit {
                    expires_at: Some(now + chrono::Duration::days(2)),
                    title: Some("Later reset".into()),
                    status: Some(BankedResetCreditStatus::Available),
                },
                BankedResetCredit {
                    expires_at: Some(now + chrono::Duration::days(1)),
                    title: Some("Earlier reset".into()),
                    status: Some(BankedResetCreditStatus::Available),
                },
            ]
        }),
        windows: (id == "positive")
            .then(|| LimitWindow {
                id: "weekly-positive".into(),
                label: "Weekly".into(),
                percent_used: 42.0,
                resets_at: Some(now + chrono::Duration::days(6)),
                severity: None,
                scope: None,
                is_active: true,
                raw: Default::default(),
            })
            .into_iter()
            .collect(),
        extras: Vec::new(),
        fetched_at: Some(now),
        source: String::new(),
        issue: None,
        status: SnapshotStatus {
            freshness: SnapshotFreshness::Live,
            last_attempted_at: Some(now),
            issue: None,
        },
    }
}

fn bounds(cx: &mut VisualTestContext, selector: &'static str) -> Bounds<Pixels> {
    cx.debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
}

fn has(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}
