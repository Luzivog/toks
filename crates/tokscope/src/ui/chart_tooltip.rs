use gpui::{div, prelude::*, px, App, Hsla, SharedString};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};

use super::{claude_accent, codex_accent, fmt_cost_full, fmt_tokens};

/// One aligned point in a cross-provider usage chart.
#[derive(Clone)]
pub(super) struct ProviderPoint {
    pub(super) heading: String,
    pub(super) label: SharedString,
    pub(super) claude: f64,
    pub(super) claude_tokens: i64,
    pub(super) codex: f64,
    pub(super) codex_tokens: i64,
}

pub(super) fn usage_tooltip_row(
    label: &'static str,
    color: Option<Hsla>,
    tokens: i64,
    cost: f64,
    emphasized: bool,
    cx: &App,
) -> gpui::Div {
    h_flex()
        .w_full()
        .min_h(px(26.))
        .items_center()
        .when(emphasized, |row| row.font_semibold())
        .child(
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_2()
                .items_center()
                .when_some(color, |row, color| {
                    row.child(div().size_2().rounded_full().bg(color).flex_shrink_0())
                })
                .child(div().text_sm().child(label)),
        )
        .child(
            div()
                .w(px(82.))
                .text_right()
                .text_sm()
                .when(!emphasized, |value| {
                    value.text_color(cx.theme().muted_foreground)
                })
                .child(fmt_tokens(tokens)),
        )
        .child(
            div()
                .w(px(78.))
                .text_right()
                .text_sm()
                .when(!emphasized, |value| {
                    value.text_color(cx.theme().muted_foreground)
                })
                .child(fmt_cost_full(cost)),
        )
}

pub(super) fn provider_rows(point: &ProviderPoint) -> Vec<(&'static str, i64, f64)> {
    let mut providers = vec![
        ("Codex", point.codex_tokens, point.codex),
        ("Claude Code", point.claude_tokens, point.claude),
    ];
    providers.retain(|(_, tokens, cost)| *tokens > 0 || *cost > 0.0);
    providers.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    providers
}

pub(super) fn usage_point_tooltip(point: &ProviderPoint, cx: &App) -> gpui::Div {
    let total_tokens = point.claude_tokens + point.codex_tokens;
    let total_cost = point.claude + point.codex;

    let mut tooltip = v_flex()
        .w(px(300.))
        .gap_2()
        .p_3()
        .child(div().text_sm().font_semibold().child(point.heading.clone()))
        .child(
            h_flex()
                .w_full()
                .pb_1()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(div().flex_1().child("Provider"))
                .child(div().w(px(82.)).text_right().child("Tokens"))
                .child(div().w(px(78.)).text_right().child("Cost")),
        );
    for (label, tokens, cost) in provider_rows(point) {
        let color = if label == "Claude Code" {
            claude_accent()
        } else {
            codex_accent()
        };
        tooltip = tooltip.child(usage_tooltip_row(
            label,
            Some(color),
            tokens,
            cost,
            false,
            cx,
        ));
    }

    tooltip
        .child(div().w_full().border_t_1().border_color(cx.theme().border))
        .child(usage_tooltip_row(
            "Total",
            None,
            total_tokens,
            total_cost,
            true,
            cx,
        ))
}
