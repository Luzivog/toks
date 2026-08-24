#![cfg(feature = "test-support")]

use chrono::{TimeZone, Utc};
use gpui::{px, size, TestAppContext};
use toks::test_support::set_page;
use toks::{Page, ToksApp};

use super::support::{usage_history, Harness};

#[gpui::test]
fn all_time_navigation_opens_summary_chart_and_model_breakdown(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, app(Page::Overview), size(px(1600.), px(1000.)));
    harness.click("all-time");

    harness.bounds("all-time-page");
    harness.bounds("all-time-usage-chart");
    harness.bounds("model-row-all-time-openai-gpt-test");
}

#[gpui::test]
fn overview_chart_and_current_rows_fit_a_narrow_window(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, app(Page::Overview), size(px(1200.), px(1800.)));
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
    let mut harness = Harness::open(cx, app(Page::Overview), size(px(1288.), px(900.)));
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
    let mut harness = Harness::open(cx, app(Page::Daily), size(px(940.), px(1200.)));

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
    let mut harness = Harness::open(cx, app(Page::Overview), size(px(2200.), px(1200.)));
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
    let mut harness = Harness::open(cx, app(Page::Overview), size(px(1000.), px(1200.)));
    let detail = harness.bounds("detail");
    let content = harness.bounds("page-content");

    assert_eq!(content.left(), detail.left());
    assert_eq!(content.right(), detail.right());
}

#[gpui::test]
fn usage_headers_place_average_cost_before_totals(cx: &mut TestAppContext) {
    let mut harness = Harness::open(cx, app(Page::Overview), size(px(1600.), px(1000.)));
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

fn app(page: Page) -> ToksApp {
    let now = Utc
        .with_ymd_and_hms(2026, 8, 18, 12, 0, 0)
        .single()
        .expect("valid fixture timestamp");
    let mut app = ToksApp::from_snapshots(Some(usage_history(now.timestamp_millis())), vec![], now);
    set_page(&mut app, page);
    app
}
