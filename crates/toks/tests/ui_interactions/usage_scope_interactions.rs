#![cfg(feature = "test-support")]

use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, Pixels, Size, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use toks::test_support::{initialize, set_page, WindowFrame};
use toks::{Page, ToksApp};
use toks_core::history::{HistorySnapshot, ModelUsage, SourceHistory, UsageBucket, UsageSeries};

struct Harness {
    cx: &'static mut VisualTestContext,
}

impl Harness {
    fn open(cx: &mut TestAppContext, viewport: Size<Pixels>) -> Self {
        Self::open_page(cx, Page::Overview, viewport)
    }

    fn open_page(cx: &mut TestAppContext, page: Page, viewport: Size<Pixels>) -> Self {
        initialize(cx);
        let now = Utc
            .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
            .single()
            .expect("valid fixture timestamp");
        let app = cx.new(|_| {
            let mut app =
                ToksApp::from_snapshots(Some(history(now.timestamp_millis())), vec![], now);
            set_page(&mut app, page);
            app
        });
        let content = app.clone();
        let window = cx.update(|cx| {
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(Bounds::new(
                        point(px(0.), px(0.)),
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
        let cx = VisualTestContext::from_window(*window.deref(), cx).into_mut();
        cx.run_until_parked();
        Self { cx }
    }

    fn bounds(&mut self, selector: &'static str) -> Bounds<Pixels> {
        self.cx
            .debug_bounds(selector)
            .unwrap_or_else(|| panic!("missing rendered selector: {selector}"))
    }

    fn has(&mut self, selector: &'static str) -> bool {
        self.cx.debug_bounds(selector).is_some()
    }

    fn click(&mut self, selector: &'static str) {
        let position = self.bounds(selector).center();
        self.cx
            .simulate_mouse_move(position, None::<MouseButton>, Modifiers::none());
        self.cx.simulate_click(position, Modifiers::none());
        self.cx.run_until_parked();
    }
}

#[gpui::test]
fn all_time_navigation_opens_summary_chart_and_model_breakdown(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1600.), px(1000.)));
    harness.click("all-time");

    harness.bounds("all-time-page");
    harness.bounds("all-time-usage-chart");
    harness.bounds("model-row-all-time-openai-gpt-test");
}

#[gpui::test]
fn overview_chart_and_current_rows_fit_a_narrow_window(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1200.), px(1800.)));
    let card = harness.bounds("overview-usage-card");
    let chart = harness.bounds("overview-usage-chart");
    let summary = harness.bounds("usage-summary-sidebar");
    let current = harness.bounds("overview-current-usage");
    let today = harness.bounds("overview-usage-today");
    let month = harness.bounds("overview-usage-month");

    assert!(card.contains(&chart.center()));
    assert!(card.contains(&current.center()));
    assert!(chart.size.width >= px(500.));
    assert_eq!(chart.size.height, summary.size.height);
    assert!(current.top() >= chart.bottom());
    assert!(current.contains(&today.center()));
    assert!(current.contains(&month.center()));
    assert!(today.left() >= current.left() && today.right() <= current.right());
    assert!(month.left() >= current.left() && month.right() <= current.right());
}

#[gpui::test]
fn overview_hides_low_priority_metrics_before_they_overflow(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1288.), px(900.)));
    let current = harness.bounds("overview-current-usage");
    let cost = harness.bounds("overview-usage-today-cost");

    assert!(!harness.has("overview-usage-today-turns"));
    assert!(!harness.has("overview-usage-today-messages"));
    assert!(harness.has("overview-usage-today-input"));
    assert!(harness.has("overview-usage-today-cache-write"));
    assert!(harness.has("overview-usage-today-cost-per-million"));
    assert!(harness.has("overview-usage-today-total"));
    assert!(current.contains(&cost.center()));
    assert!(cost.right() <= current.right());
}

#[gpui::test]
fn minimum_window_uses_the_same_columns_for_headers_and_rows(cx: &mut TestAppContext) {
    let mut harness = Harness::open_page(cx, Page::Daily, size(px(940.), px(1200.)));

    for column in ["turns", "messages", "reasoning"] {
        let header: &'static str = Box::leak(format!("usage-sort-daily-{column}").into_boxed_str());
        let cell: &'static str =
            Box::leak(format!("usage-row-daily-2026-08-18-{column}").into_boxed_str());
        assert!(!harness.has(header));
        assert!(!harness.has(cell));
    }
    for column in ["input", "cache-write", "cost-per-million", "total", "cost"] {
        let header: &'static str = Box::leak(format!("usage-sort-daily-{column}").into_boxed_str());
        let cell: &'static str =
            Box::leak(format!("usage-row-daily-2026-08-18-{column}").into_boxed_str());
        assert!(harness.has(header));
        assert!(harness.has(cell));
    }

    assert!(!harness.has("model-sort-daily-turns"));
    assert!(harness.has("model-sort-daily-messages"));
    assert!(harness.has("model-sort-daily-cost"));
    assert!(
        harness.bounds("model-sort-daily-cost").right() <= harness.bounds("page-content").right()
    );
}

#[gpui::test]
fn wide_pages_use_a_centered_readable_content_column(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(2200.), px(1200.)));
    let detail = harness.bounds("detail");
    let content = harness.bounds("page-content");
    let left_gutter = content.left() - detail.left();
    let right_gutter = detail.right() - content.right();

    assert_eq!(content.size.width, px(1280.));
    assert!((left_gutter - right_gutter).abs() <= px(1.));
    assert!(left_gutter > px(100.));
}

#[gpui::test]
fn narrow_pages_keep_the_full_available_content_width(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1000.), px(1200.)));
    let detail = harness.bounds("detail");
    let content = harness.bounds("page-content");

    assert_eq!(content.left(), detail.left());
    assert_eq!(content.right(), detail.right());
}

#[gpui::test]
fn usage_headers_place_average_cost_before_totals(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1600.), px(1000.)));
    harness.click("daily");

    let output = harness.bounds("usage-sort-daily-output");
    let reasoning = harness.bounds("usage-sort-daily-reasoning");
    let cache_write = harness.bounds("usage-sort-daily-cache-write");
    let average = harness.bounds("usage-sort-daily-cost-per-million");
    let total = harness.bounds("usage-sort-daily-total");
    let cost = harness.bounds("usage-sort-daily-cost");

    assert!(output.left() < reasoning.left());
    assert!(reasoning.left() < cache_write.left());
    assert!(cache_write.left() < average.left());
    assert!(average.left() < total.left());
    assert!(total.left() < cost.left());
}

fn history(generated_at_ms: i64) -> HistorySnapshot {
    let model = ModelUsage {
        model: "gpt-test".into(),
        provider: "openai".into(),
        input: 40,
        output: 10,
        tokens: 50,
        messages: 2,
        turns: 1,
        cost: 0.5,
        ..Default::default()
    };
    let monthly = vec![
        bucket("2026-06", 20, model.clone()),
        bucket("2026-08", 50, model.clone()),
    ];
    let daily = vec![bucket("2026-08-18", 50, model.clone())];
    let source_usage = UsageSeries {
        daily: daily.clone(),
        monthly: monthly.clone(),
        ..Default::default()
    };
    let source = SourceHistory {
        client: "codex".into(),
        total_tokens: 70,
        total_cost: 0.7,
        models: vec![model],
        usage: source_usage,
        ..Default::default()
    };
    HistorySnapshot {
        sources: vec![source],
        usage: UsageSeries {
            daily,
            monthly,
            ..Default::default()
        },
        generated_at_ms,
        ..Default::default()
    }
}

fn bucket(key: &str, tokens: i64, model: ModelUsage) -> UsageBucket {
    UsageBucket {
        key: key.into(),
        tokens,
        cost: tokens as f64 / 100.0,
        models: vec![model],
        ..Default::default()
    }
}
