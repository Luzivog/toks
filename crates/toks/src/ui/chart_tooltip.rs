use gpui::{div, prelude::*, px, App, Hsla, SharedString};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::{ClientId, ProviderVisibility, USAGE_PROVIDERS};

use super::{accent_for_usage_provider, fmt_cost_full, fmt_tokens, usage_provider_label};

/// One aligned point in a cross-provider usage chart.
#[derive(Clone)]
pub(super) struct ProviderPoint {
    pub(super) heading: String,
    pub(super) label: SharedString,
    pub(super) claude: f64,
    pub(super) claude_tokens: i64,
    pub(super) codex: f64,
    pub(super) codex_tokens: i64,
    pub(super) opencode: f64,
    pub(super) opencode_tokens: i64,
}

impl ProviderPoint {
    pub(super) fn provider(&self, provider: ClientId) -> (f64, i64) {
        match provider {
            ClientId::Codex => (self.codex, self.codex_tokens),
            ClientId::Claude => (self.claude, self.claude_tokens),
            ClientId::OpenCode => (self.opencode, self.opencode_tokens),
            _ => (0.0, 0),
        }
    }
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

pub(super) fn provider_rows(
    point: &ProviderPoint,
    visibility: &ProviderVisibility,
) -> Vec<(&'static str, Hsla, i64, f64)> {
    let mut providers: Vec<_> = USAGE_PROVIDERS
        .iter()
        .copied()
        .filter(|provider| visibility.is_visible(*provider))
        .map(|provider| {
            let (cost, tokens) = point.provider(provider);
            (
                usage_provider_label(provider),
                accent_for_usage_provider(provider),
                tokens,
                cost,
            )
        })
        .collect();
    providers.retain(|&(_, _, tokens, cost)| tokens > 0 || cost > 0.0);
    providers.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    providers
}

pub(super) fn usage_point_tooltip(
    point: &ProviderPoint,
    visibility: &ProviderVisibility,
    cx: &App,
) -> gpui::Div {
    let providers = provider_rows(point, visibility);
    let total_tokens = providers
        .iter()
        .fold(0_i64, |total, row| total.saturating_add(row.2));
    let total_cost = providers.iter().map(|row| row.3).sum();

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
    for (label, color, tokens, cost) in providers {
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
