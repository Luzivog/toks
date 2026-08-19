#![cfg(feature = "test-support")]
use chrono::{NaiveDate, TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Entity, Modifiers, MouseButton, Pixels, Size,
    TestAppContext, VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions,
};
use gpui_component::TitleBar;
use std::ops::Deref;
use toks::test_support::{initialize, set_page, sidebar_open, WindowAction, WindowFrame};
use toks::{Page, ToksApp};
use toks_core::accounts::{AccountIdentityKind, AccountSource, CredentialProfileKind};
use toks_core::history::{
    DaySlice, HistorySnapshot, MinuteSlice, ModelUsage, SourceHistory, UsageBucket, UsageSeries,
};
use toks_core::limits::{LimitWindow, SnapshotFreshness, SnapshotStatus};
use toks_core::{LimitSnapshot, Provider, ProviderAccount};
const VIEWPORT: Size<Pixels> = size(px(1600.0), px(1800.0));
struct Harness {
    app: Entity<ToksApp>,
    frame: Entity<WindowFrame>,
    cx: &'static mut VisualTestContext,
}
impl Harness {
    fn open(cx: &mut TestAppContext, page: Page, viewport: Size<Pixels>) -> Self {
        Self::open_with_limits(cx, page, viewport, Vec::new())
    }
    fn open_with_limits(
        cx: &mut TestAppContext,
        page: Page,
        viewport: Size<Pixels>,
        limits: Vec<LimitSnapshot>,
    ) -> Self {
        initialize(cx);
        let app = cx.new(|_| {
            let mut app = ToksApp::from_snapshots(
                Some(fixture_history()),
                limits,
                Utc.with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
                    .single()
                    .expect("valid fixture timestamp"),
            );
            set_page(&mut app, page);
            app
        });
        let content = app.clone();
        let window = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.0), px(0.0)),
                        viewport,
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
        let frame = window.root(cx).expect("window has a root frame");
        let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
        cx.run_until_parked();
        Self { app, frame, cx }
    }
    fn bounds(&mut self, selector: &'static str) -> Bounds<Pixels> {
        self.cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
    }
    fn has(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }
    fn move_to(&mut self, selector: &'static str) {
        let position = self.bounds(selector).center();
        self.cx
            .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
    }
    fn click(&mut self, selector: &'static str) {
        self.move_to("resize-right");
        let position = self.bounds(selector).center();
        self.cx
            .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
        self.cx.simulate_click(position, Modifiers::none());
        self.cx.run_until_parked();
    }
    fn above(&mut self, first: &'static str, second: &'static str) -> bool {
        self.bounds(first).center().y < self.bounds(second).center().y
    }
}
#[gpui::test]
fn account_handle_drops_onto_another_account_row(cx: &mut TestAppContext) {
    let previous_data_home = std::env::var_os("XDG_DATA_HOME");
    let data_home =
        std::env::temp_dir().join(format!("toks-account-drag-test-{}", std::process::id()));
    std::fs::create_dir_all(&data_home).expect("temporary XDG data home");
    std::env::set_var("XDG_DATA_HOME", &data_home);
    let limits = vec![
        limit_snapshot(Provider::Codex, "first"),
        limit_snapshot(Provider::Claude, "second"),
    ];
    let mut harness = Harness::open_with_limits(cx, Page::Overview, VIEWPORT, limits);
    assert!(harness.above("account-drop-codex-first", "account-drop-claude-second"));

    let start = harness.bounds("account-drag-codex-first").center();
    let end = harness.bounds("account-drop-claude-second").center();
    harness
        .cx
        .simulate_mouse_move(start, None::<MouseButton>, Modifiers::none());
    harness
        .cx
        .simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    harness.cx.simulate_mouse_move(
        point(start.x + px(12.), start.y),
        MouseButton::Left,
        Modifiers::none(),
    );
    harness
        .cx
        .simulate_mouse_move(end, MouseButton::Left, Modifiers::none());
    harness
        .cx
        .simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    harness.cx.run_until_parked();

    assert!(harness.above("account-drop-claude-second", "account-drop-codex-first"));
    if let Some(previous) = previous_data_home {
        std::env::set_var("XDG_DATA_HOME", previous);
    } else {
        std::env::remove_var("XDG_DATA_HOME");
    }
    std::fs::remove_dir_all(data_home).expect("temporary account order is removable");
}

#[gpui::test]
fn plan_limits_render_as_distinct_account_and_quota_rows(cx: &mut TestAppContext) {
    let limits = vec![limit_snapshot(Provider::Codex, "readable")];
    let mut harness = Harness::open_with_limits(cx, Page::Overview, VIEWPORT, limits);

    assert!(harness.has("account-group-codex-readable"));
    assert!(harness.has("account-status-codex-readable"));
    let quota = harness.bounds("quota-row-weekly");
    assert!(quota.size.height >= px(40.));
}

#[gpui::test]
fn add_account_opens_the_provider_menu_after_crossing_a_resize_edge(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, Page::Overview, VIEWPORT);
    assert!(!harness.has("add-account-provider-claude"));

    harness.click("add-account");

    assert!(harness.has("add-account-provider-claude"));
    assert!(harness.has("add-account-provider-codex"));
}

#[gpui::test]
fn compact_sidebar_backdrop_closes_the_overlay(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, Page::Overview, size(px(1000.0), px(900.0)));
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));

    harness.click("toggle-sidebar");
    assert!(harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
    assert!(harness.has("sidebar-dismiss"));

    harness.click("sidebar-dismiss");
    assert!(!harness
        .app
        .read_with(harness.cx, |app, _| sidebar_open(app)));
}

#[gpui::test]
fn hourly_usage_actions_survive_the_resize_edge(cx: &mut TestAppContext) {
    assert_usage_actions(
        Harness::open(cx, Page::Hourly, VIEWPORT),
        "hourly",
        "usage-row-hourly-2026-08-18 00:00",
        "usage-row-hourly-2026-08-18 02:00",
        "usage-row-hourly-2026-08-18 11:00",
    );
}

#[gpui::test]
fn daily_usage_actions_survive_the_resize_edge(cx: &mut TestAppContext) {
    assert_usage_actions(
        Harness::open(cx, Page::Daily, VIEWPORT),
        "daily",
        "usage-row-daily-2026-08-18",
        "usage-row-daily-2026-08-08",
        "usage-row-daily-2026-08-18",
    );
}

#[gpui::test]
fn sort_headers_keep_the_arrow_with_the_label_and_toggle_time(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, Page::Hourly, VIEWPORT);
    let earlier = "usage-row-hourly-2026-08-18 02:00";
    let later = "usage-row-hourly-2026-08-18 03:00";
    let header_before = harness.bounds("model-sort-hourly-cache-write");
    let label_before = harness.bounds("model-sort-hourly-cache-write-label");
    harness.click("model-sort-hourly-cache-write");
    let header_after = harness.bounds("model-sort-hourly-cache-write");
    let label_after = harness.bounds("model-sort-hourly-cache-write-label");
    let arrow = harness.bounds("model-sort-hourly-cache-write-indicator");
    assert_eq!(header_before, header_after);
    assert_eq!(label_before.origin, label_after.origin);
    assert!(
        header_after.contains(&arrow.center()),
        "header {header_after:?}, arrow {arrow:?}, label {label_after:?}"
    );
    assert!(arrow.origin.x + arrow.size.width <= label_after.origin.x);
    harness.click("usage-sort-hourly-period");
    assert!(harness.has("usage-sort-hourly-period-indicator"));
    assert!(harness.has("usage-day-2026-08-18"));
    assert!(harness.above(later, earlier));
    harness.click("usage-sort-hourly-period");
    assert!(harness.above(earlier, later));
}

#[gpui::test]
fn monthly_usage_actions_survive_the_resize_edge(cx: &mut TestAppContext) {
    assert_usage_actions(
        Harness::open(cx, Page::Monthly, VIEWPORT),
        "monthly",
        "usage-row-monthly-2026-08",
        "usage-row-monthly-2025-10",
        "usage-row-monthly-2026-08",
    );
}

fn assert_usage_actions(
    mut harness: Harness,
    period: &'static str,
    initially_hidden: &'static str,
    high_row: &'static str,
    low_row: &'static str,
) {
    let more = match period {
        "hourly" => "hourly-usage-more",
        "daily" => "daily-usage-more",
        "monthly" => "monthly-usage-more",
        _ => unreachable!("known usage period"),
    };
    assert!(!harness.has(initially_hidden));
    harness.click(more);
    assert!(harness.has(initially_hidden));

    for column in [
        "turns",
        "messages",
        "input",
        "output",
        "cache-read",
        "total",
        "cost",
        "cost-per-million",
    ] {
        let selector: &'static str =
            Box::leak(format!("usage-sort-{period}-{column}").into_boxed_str());
        harness.click(selector);
        assert!(harness.above(high_row, low_row), "{selector} descending");
        harness.click(selector);
        assert!(harness.above(low_row, high_row), "{selector} ascending");
    }
}

#[gpui::test]
fn hourly_model_headers_are_clickable(cx: &mut TestAppContext) {
    assert_model_actions(Harness::open(cx, Page::Hourly, VIEWPORT), "hourly");
}

#[gpui::test]
fn daily_model_headers_are_clickable(cx: &mut TestAppContext) {
    assert_model_actions(Harness::open(cx, Page::Daily, VIEWPORT), "daily");
}

#[gpui::test]
fn monthly_model_headers_are_clickable(cx: &mut TestAppContext) {
    assert_model_actions(Harness::open(cx, Page::Monthly, VIEWPORT), "monthly");
}

fn assert_model_actions(mut harness: Harness, page: &'static str) {
    let high: &'static str = Box::leak(format!("model-row-{page}-openai-large").into_boxed_str());
    let low: &'static str = Box::leak(format!("model-row-{page}-openai-small").into_boxed_str());
    for column in [
        "input",
        "cache-read",
        "cache-write",
        "output",
        "reasoning",
        "messages",
        "turns",
        "total",
        "cost",
    ] {
        let selector: &'static str =
            Box::leak(format!("model-sort-{page}-{column}").into_boxed_str());
        harness.click(selector);
        assert!(harness.above(high, low), "{selector} descending");
        harness.click(selector);
        assert!(harness.above(low, high), "{selector} ascending");
    }
}

#[gpui::test]
fn window_controls_receive_clicks_after_every_resize_zone(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, Page::Overview, size(px(1320.0), px(860.0)));
    for (selector, expected) in [
        ("window-minimize", WindowAction::Minimize),
        ("window-maximize", WindowAction::ToggleMaximize),
        ("window-close", WindowAction::Close),
    ] {
        for edge in [
            "resize-top",
            "resize-right",
            "resize-bottom",
            "resize-left",
            "resize-top-left",
            "resize-top-right",
            "resize-bottom-left",
            "resize-bottom-right",
        ] {
            harness.move_to(edge);
            let position = harness.bounds(selector).center();
            harness
                .cx
                .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
            harness.cx.simulate_click(position, Modifiers::none());
            harness.cx.run_until_parked();
            assert_eq!(
                harness
                    .frame
                    .read_with(harness.cx, |frame, _| frame.observed_action()),
                Some(expected),
                "{selector} after {edge}",
            );
        }
    }
}

fn fixture_history() -> HistorySnapshot {
    let models = fixture_models();
    let hourly = (0..12)
        .map(|hour| {
            let value = match hour {
                2 => 1_000,
                11 => 1,
                _ => 20 + hour,
            };
            bucket(format!("2026-08-18 {hour:02}:00"), value, models.clone())
        })
        .collect::<Vec<_>>();
    let start = NaiveDate::from_ymd_opt(2026, 8, 7).expect("valid fixture date");
    let daily = (0..12)
        .map(|offset| {
            let date = start + chrono::Duration::days(offset);
            let value = match offset {
                1 => 1_000,
                11 => 1,
                _ => 20 + offset,
            };
            bucket(date.format("%Y-%m-%d").to_string(), value, models.clone())
        })
        .collect::<Vec<_>>();
    let month_keys = [
        "2025-09", "2025-10", "2025-11", "2025-12", "2026-01", "2026-02", "2026-03", "2026-04",
        "2026-05", "2026-06", "2026-07", "2026-08",
    ];
    let monthly = month_keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let value = match index {
                1 => 1_000,
                11 => 1,
                _ => 20 + index as i64,
            };
            bucket((*key).to_string(), value, models.clone())
        })
        .collect::<Vec<_>>();
    let usage = UsageSeries {
        daily,
        hourly,
        monthly,
    };
    let generated_at_ms = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp")
        .timestamp_millis();
    let source = SourceHistory {
        client: "codex".into(),
        minutes: vec![MinuteSlice {
            minute: generated_at_ms.div_euclid(60_000),
            tokens: 10_000,
            cost: 10.0,
            models: models.clone(),
        }],
        days: vec![DaySlice {
            date: "2026-08-18".into(),
            tokens: 10_000,
            cost: 10.0,
            messages: 100,
        }],
        usage: usage.clone(),
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![source],
        usage,
        generated_at_ms,
        ..Default::default()
    }
}

fn fixture_models() -> Vec<ModelUsage> {
    vec![model("large", 1_000), model("small", 1)]
}

fn model(name: &str, value: i64) -> ModelUsage {
    ModelUsage {
        model: name.into(),
        provider: "openai".into(),
        input: value,
        output: value,
        cache_read: value,
        cache_write: value,
        reasoning: value,
        tokens: value.saturating_mul(5),
        messages: value,
        turns: value,
        cost: (value as f64).powi(2),
        ..Default::default()
    }
}

fn bucket(key: String, value: i64, models: Vec<ModelUsage>) -> UsageBucket {
    UsageBucket {
        key,
        input: value,
        output: value,
        cache_read: value,
        cache_write: value,
        reasoning: value,
        tokens: value.saturating_mul(5),
        messages: value,
        turns: value,
        cost: (value as f64).powi(2),
        models,
        ..Default::default()
    }
}

fn limit_snapshot(provider: Provider, id: &str) -> LimitSnapshot {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    LimitSnapshot {
        provider,
        account: ProviderAccount {
            id: id.into(),
            identity_kind: AccountIdentityKind::ProviderPrincipal,
            email: Some(format!("{id}@example.test")),
            sources: vec![AccountSource {
                profile_id: format!("profile-{id}").into(),
                kind: CredentialProfileKind::Managed,
                primary: true,
            }],
        },
        plan: None,
        plan_multiplier: None,
        windows: vec![LimitWindow {
            id: "weekly".into(),
            label: "Weekly — GPT-5.3-Codex-Spark".into(),
            percent_used: 42.0,
            resets_at: Some(now + chrono::Duration::days(6)),
            severity: None,
            scope: Some("GPT-5.3-Codex-Spark".into()),
            is_active: true,
            raw: Default::default(),
        }],
        extras: Vec::new(),
        fetched_at: Some(now - chrono::Duration::minutes(2)),
        source: String::new(),
        issue: None,
        status: SnapshotStatus {
            freshness: SnapshotFreshness::Live,
            last_attempted_at: Some(now),
            issue: None,
        },
    }
}
