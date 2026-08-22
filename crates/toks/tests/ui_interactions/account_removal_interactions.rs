#![cfg(feature = "test-support")]

use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, TestAppContext, VisualTestContext,
    WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::test_support::{initialize, WindowFrame};
use toks::ToksApp;
use toks_core::accounts::{
    AccountId, AccountIdentityKind, AccountSource, CredentialProfileId, CredentialProfileKind,
};
use toks_core::limits::{SnapshotFreshness, SnapshotStatus};
use toks_core::{LimitSnapshot, Provider, ProviderAccount};

#[gpui::test]
fn removal_actions_use_opaque_account_ids_and_confirm_before_removing(cx: &mut TestAppContext) {
    initialize(cx);
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let app = cx.new(|_| {
        ToksApp::from_snapshots(
            None,
            vec![
                snapshot("logical-first", "same@example.test", now),
                snapshot("logical-second", "same@example.test", now),
            ],
            now,
        )
    });
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

    assert!(has(cx, "account-actions-codex-logical-first"));
    assert!(has(cx, "account-actions-codex-logical-second"));
    assert!(!has(cx, "account-actions-codex-same@example.test"));

    click(cx, "account-actions-codex-logical-first");
    assert!(has(cx, "remove-account-codex-logical-first"));
    click(cx, "remove-account-codex-logical-first");
    assert!(has(cx, "account-removal-confirmation"));
    assert!(has(cx, "account-removal-confirmation-copy"));

    // Opening the confirmation must not mutate the row before explicit consent.
    assert!(has(cx, "account-group-codex-logical-first"));
    assert!(has(cx, "account-group-codex-logical-second"));
}

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let position = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
        .center();
    cx.simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
    cx.simulate_click(position, Modifiers::none());
    cx.run_until_parked();
}

fn has(cx: &mut VisualTestContext, selector: &'static str) -> bool {
    cx.debug_bounds(selector).is_some()
}

fn snapshot(id: &str, email: &str, now: chrono::DateTime<Utc>) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account: ProviderAccount {
            id: AccountId::new(id),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some(email.into()),
            sources: vec![AccountSource {
                profile_id: CredentialProfileId::new(format!("profile-{id}")),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
        plan: Some("Pro".into()),
        plan_multiplier: None,
        banked_resets: 0,
        banked_reset_credits: None,
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
    }
}
