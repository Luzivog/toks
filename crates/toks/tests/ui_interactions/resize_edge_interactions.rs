use gpui::{px, size, Modifiers, MouseButton, TestAppContext};
use toks::test_support::WindowAction;
use toks::Page;

use super::support::{Harness, VIEWPORT};

#[gpui::test]
fn add_account_opens_the_provider_menu_after_crossing_a_resize_edge(cx: &mut TestAppContext) {
    let mut harness = Harness::open_page(cx, Page::Overview, VIEWPORT);
    assert!(!harness.has("add-account-provider-claude"));

    harness.click_after_resize_edge("add-account");

    assert!(harness.has("add-account-provider-claude"));
    assert!(harness.has("add-account-provider-codex"));
}

#[gpui::test]
fn hourly_usage_actions_survive_the_resize_edge(cx: &mut TestAppContext) {
    assert_usage_actions(
        Harness::open_page(cx, Page::Hourly, VIEWPORT),
        "hourly",
        "usage-row-hourly-2026-08-18 00:00",
        "usage-row-hourly-2026-08-18 02:00",
        "usage-row-hourly-2026-08-18 11:00",
    );
}

#[gpui::test]
fn daily_usage_actions_survive_the_resize_edge(cx: &mut TestAppContext) {
    assert_usage_actions(
        Harness::open_page(cx, Page::Daily, VIEWPORT),
        "daily",
        "usage-row-daily-2026-08-18",
        "usage-row-daily-2026-08-08",
        "usage-row-daily-2026-08-18",
    );
}

#[gpui::test]
fn sort_headers_keep_the_arrow_with_the_label_and_toggle_time(cx: &mut TestAppContext) {
    let mut harness = Harness::open_page(cx, Page::Hourly, VIEWPORT);
    let earlier = "usage-row-hourly-2026-08-18 02:00";
    let later = "usage-row-hourly-2026-08-18 03:00";
    let header_before = harness.bounds("model-sort-hourly-cache-write");
    let label_before = harness.bounds("model-sort-hourly-cache-write-label");
    harness.click_after_resize_edge("model-sort-hourly-cache-write");
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
    harness.click_after_resize_edge("usage-sort-hourly-period");
    assert!(harness.has("usage-sort-hourly-period-indicator"));
    assert!(harness.has("usage-day-2026-08-18"));
    assert!(harness.above(later, earlier));
    harness.click_after_resize_edge("usage-sort-hourly-period");
    assert!(harness.above(earlier, later));
}

#[gpui::test]
fn monthly_usage_actions_survive_the_resize_edge(cx: &mut TestAppContext) {
    assert_usage_actions(
        Harness::open_page(cx, Page::Monthly, VIEWPORT),
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
    harness.click_after_resize_edge(more);
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
        harness.click_after_resize_edge(selector);
        assert!(harness.above(high_row, low_row), "{selector} descending");
        harness.click_after_resize_edge(selector);
        assert!(harness.above(low_row, high_row), "{selector} ascending");
    }
}

#[gpui::test]
fn hourly_model_headers_are_clickable(cx: &mut TestAppContext) {
    assert_model_actions(Harness::open_page(cx, Page::Hourly, VIEWPORT), "hourly");
}

#[gpui::test]
fn daily_model_headers_are_clickable(cx: &mut TestAppContext) {
    assert_model_actions(Harness::open_page(cx, Page::Daily, VIEWPORT), "daily");
}

#[gpui::test]
fn monthly_model_headers_are_clickable(cx: &mut TestAppContext) {
    assert_model_actions(Harness::open_page(cx, Page::Monthly, VIEWPORT), "monthly");
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
        harness.click_after_resize_edge(selector);
        assert!(harness.above(high, low), "{selector} descending");
        harness.click_after_resize_edge(selector);
        assert!(harness.above(low, high), "{selector} ascending");
    }
}

#[gpui::test]
fn window_controls_receive_clicks_after_every_resize_zone(cx: &mut TestAppContext) {
    let mut harness = Harness::open_page(cx, Page::Overview, size(px(1320.0), px(860.0)));
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
