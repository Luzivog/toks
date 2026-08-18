#![cfg(feature = "test-support")]

use std::ops::Deref;

use chrono::{TimeZone, Utc};
use gpui::{
    point, px, size, AppContext, Bounds, Modifiers, MouseButton, Pixels, Size, TestAppContext,
    VisualTestContext, WindowBackgroundAppearance, WindowBounds, WindowDecorations, WindowOptions,
};
use gpui_component::TitleBar;
use tokscope::test_support::{initialize, WindowFrame};
use tokscope::TokscopeApp;
use tokscope_core::history::{
    HistorySnapshot, ModelUsage, SourceHistory, UsageBucket, UsageSeries,
};

struct Harness {
    cx: &'static mut VisualTestContext,
}

impl Harness {
    fn open(cx: &mut TestAppContext, viewport: Size<Pixels>) -> Self {
        initialize(cx);
        let now = Utc
            .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
            .single()
            .expect("valid fixture timestamp");
        let app = cx.new(|_| {
            TokscopeApp::from_snapshots(Some(history(now.timestamp_millis())), vec![], now)
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
fn overview_ranges_share_a_row_when_both_remain_readable(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1900.), px(1000.)));
    let today = harness.bounds("overview-today-card");
    let month = harness.bounds("overview-month-card");

    assert_eq!(today.top(), month.top());
    assert!(today.size.width >= px(700.));
    assert!(month.size.width >= px(700.));
}

#[gpui::test]
fn overview_ranges_stack_in_a_narrow_window(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, size(px(1200.), px(1800.)));
    let today = harness.bounds("overview-today-card");
    let month = harness.bounds("overview-month-card");
    let today_chart = harness.bounds("overview-today-chart");
    let month_chart = harness.bounds("overview-month-chart");

    assert!(month.top() >= today.bottom());
    assert!(today.contains(&today_chart.center()));
    assert!(month.contains(&month_chart.center()));
    assert!(today_chart.size.width >= px(500.));
    assert!(month_chart.size.width >= px(500.));
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
