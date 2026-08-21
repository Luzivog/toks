use gpui::{div, prelude::*, px, relative, App, SharedString};
use gpui_component::{chart::AreaChart, tooltip::Tooltip, ActiveTheme};

use super::{
    chart_tooltip::ProviderPoint, claude_accent, codex_accent, opencode_accent, usage_point_tooltip,
};

const USAGE_AXIS_GAP: f32 = 18.0;
const USAGE_CHART_TOP_GAP: f32 = 10.0;

fn top_provider_series(point: &ProviderPoint) -> (f64, gpui::Hsla) {
    let candidates = [
        (point.claude_tokens.max(0) as f64, claude_accent()),
        (point.codex_tokens.max(0) as f64, codex_accent()),
        (point.opencode_tokens.max(0) as f64, opencode_accent()),
    ];
    candidates
        .into_iter()
        .max_by(|left, right| {
            left.0
                .partial_cmp(&right.0)
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or_default()
}

/// Match gpui-component's AreaChart scale so the hover marker sits directly
/// on the selected day's higher series.
pub(super) fn usage_marker_top(value: f64, maximum: f64) -> f32 {
    if maximum > 0.0 {
        (1.0 - (value / maximum) as f32).clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Return the hit region and the point's x-position inside it. Regions meet
/// halfway between days, producing a nearest-day snap without painting them.
pub(super) fn usage_hover_geometry(index: usize, count: usize) -> (f32, f32, f32) {
    if count <= 1 {
        return (0.0, 1.0, 0.5);
    }
    let step = 1.0 / (count - 1) as f32;
    let point = index as f32 * step;
    let left = if index == 0 { 0.0 } else { point - step / 2.0 };
    let right = if index + 1 == count {
        1.0
    } else {
        point + step / 2.0
    };
    let point_in_region = (point - left) / (right - left);
    (left, right - left, point_in_region)
}

pub(super) fn usage_chart_maximum(data: &[ProviderPoint]) -> f64 {
    data.iter()
        .flat_map(|point| {
            [
                point.claude_tokens,
                point.codex_tokens,
                point.opencode_tokens,
            ]
        })
        .map(|value| value.max(0) as f64)
        .fold(0.0_f64, f64::max)
}

pub(super) fn provider_usage_chart(
    data: Vec<ProviderPoint>,
    id_prefix: &'static str,
    cx: &App,
) -> gpui::Div {
    let maximum = usage_chart_maximum(&data);
    let point_count = data.len();
    let hover_targets: Vec<_> = data
        .iter()
        .cloned()
        .enumerate()
        .filter_map(|(index, point)| {
            if point.claude <= 0.0
                && point.codex <= 0.0
                && point.opencode <= 0.0
                && point.claude_tokens <= 0
                && point.codex_tokens <= 0
                && point.opencode_tokens <= 0
            {
                return None;
            }
            let (left, width, marker_x) = usage_hover_geometry(index, point_count);
            let (marker_value, marker_color) = top_provider_series(&point);
            let marker_y = usage_marker_top(marker_value, maximum);
            let group: SharedString = format!("{id_prefix}-point-{index}").into();
            Some(
                div()
                    .group(group.clone())
                    .id((id_prefix, index))
                    .absolute()
                    .top(px(USAGE_CHART_TOP_GAP))
                    .bottom(px(USAGE_AXIS_GAP))
                    .left(relative(left))
                    .w(relative(width))
                    .tooltip(move |window, cx| {
                        let point = point.clone();
                        Tooltip::element(move |_, cx| usage_point_tooltip(&point, cx))
                            .p_0()
                            .build(window, cx)
                    })
                    .child(
                        div()
                            .absolute()
                            .left(relative(marker_x))
                            .top(relative(marker_y))
                            .ml(px(-5.))
                            .mt(px(-5.))
                            .size(px(10.))
                            .rounded_full()
                            .border_2()
                            .border_color(cx.theme().background)
                            .bg(marker_color)
                            .invisible()
                            .group_hover(group, |marker| marker.visible()),
                    ),
            )
        })
        .collect();

    div()
        .debug_selector(move || format!("{id_prefix}-chart"))
        .relative()
        .w_full()
        .min_w_0()
        .child(
            AreaChart::new(data)
                .x(|point: &ProviderPoint| point.label.clone())
                .y(|point: &ProviderPoint| point.claude_tokens.max(0) as f64)
                .stroke(claude_accent())
                .fill(claude_accent().opacity(0.22))
                .linear()
                .y(|point: &ProviderPoint| point.codex_tokens.max(0) as f64)
                .stroke(codex_accent())
                .fill(codex_accent().opacity(0.12))
                .linear()
                .y(|point: &ProviderPoint| point.opencode_tokens.max(0) as f64)
                .stroke(opencode_accent())
                .fill(opencode_accent().opacity(0.12))
                .linear()
                .tick_margin(7),
        )
        .child(
            div()
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .children(hover_targets),
        )
}
