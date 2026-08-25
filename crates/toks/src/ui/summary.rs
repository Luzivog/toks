use gpui::{div, prelude::*, px, App};
use gpui_component::{h_flex, progress::Progress, v_flex, ActiveTheme, StyledExt};
use toks_core::{ClientId, ProviderVisibility, USAGE_PROVIDERS};

use super::{
    accent_for_usage_provider, chart_tooltip::ProviderPoint, fmt_cost_full, fmt_tokens,
    usage_provider_label,
};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct UsageSummary {
    pub(super) claude_cost: f64,
    pub(super) claude_tokens: i64,
    pub(super) codex_cost: f64,
    pub(super) codex_tokens: i64,
    pub(super) opencode_cost: f64,
    pub(super) opencode_tokens: i64,
}

impl UsageSummary {
    pub(super) fn from_points(data: &[ProviderPoint]) -> Self {
        data.iter().fold(Self::default(), |mut summary, point| {
            summary.claude_cost += point.claude;
            summary.claude_tokens = summary.claude_tokens.saturating_add(point.claude_tokens);
            summary.codex_cost += point.codex;
            summary.codex_tokens = summary.codex_tokens.saturating_add(point.codex_tokens);
            summary.opencode_cost += point.opencode;
            summary.opencode_tokens = summary
                .opencode_tokens
                .saturating_add(point.opencode_tokens);
            summary
        })
    }

    pub(super) fn exact(
        claude_cost: f64,
        claude_tokens: i64,
        codex_cost: f64,
        codex_tokens: i64,
        opencode_cost: f64,
        opencode_tokens: i64,
    ) -> Self {
        Self {
            claude_cost: claude_cost.max(0.0),
            claude_tokens: claude_tokens.max(0),
            codex_cost: codex_cost.max(0.0),
            codex_tokens: codex_tokens.max(0),
            opencode_cost: opencode_cost.max(0.0),
            opencode_tokens: opencode_tokens.max(0),
        }
    }

    fn provider(&self, provider: ClientId) -> (f64, i64) {
        match provider {
            ClientId::Codex => (self.codex_cost, self.codex_tokens),
            ClientId::Claude => (self.claude_cost, self.claude_tokens),
            ClientId::OpenCode => (self.opencode_cost, self.opencode_tokens),
            _ => (0.0, 0),
        }
    }
}

pub(super) fn usage_summary_sidebar(
    summary: UsageSummary,
    visibility: &ProviderVisibility,
    eyebrow: &'static str,
    cx: &App,
) -> gpui::Div {
    let mut providers: Vec<_> = USAGE_PROVIDERS
        .iter()
        .copied()
        .filter(|provider| visibility.is_visible(*provider))
        .map(|provider| {
            let (cost, tokens) = summary.provider(provider);
            (
                provider,
                usage_provider_label(provider),
                accent_for_usage_provider(provider),
                cost,
                tokens,
            )
        })
        .collect();
    providers.sort_by(|a, b| b.3.partial_cmp(&a.3).unwrap_or(std::cmp::Ordering::Equal));
    let total_cost = providers.iter().map(|provider| provider.3).sum();
    let total_tokens = providers
        .iter()
        .fold(0_i64, |total, provider| total.saturating_add(provider.4));
    let cost = fmt_cost_full(total_cost);

    let mut provider_bars = v_flex().gap_5();
    for (provider, name, color, cost, tokens) in providers {
        let share = if total_cost > 0.0 {
            cost / total_cost * 100.0
        } else if total_tokens > 0 {
            tokens as f64 / total_tokens as f64 * 100.0
        } else {
            0.0
        };
        provider_bars = provider_bars.child(
            v_flex()
                .debug_selector(move || format!("usage-summary-provider-{}", provider.as_str()))
                .gap_1()
                .child(
                    h_flex()
                        .justify_between()
                        .items_center()
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(div().size_2().rounded_full().bg(color))
                                .child(div().text_sm().child(name)),
                        )
                        .child(div().text_sm().font_semibold().child(fmt_cost_full(cost))),
                )
                .child(Progress::new().value(share as f32).bg(color))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!("{share:.1}% · {} tokens", fmt_tokens(tokens))),
                ),
        );
    }

    v_flex()
        .debug_selector(|| "usage-summary-sidebar".to_string())
        .w(px(290.))
        .flex_shrink_0()
        .gap_6()
        .child(
            h_flex()
                .items_start()
                .gap_5()
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child(eyebrow),
                        )
                        .child(div().text_2xl().font_bold().child(cost)),
                )
                .child(
                    v_flex()
                        .w(px(100.))
                        .flex_shrink_0()
                        .gap_1()
                        .child(
                            div()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .child("TOTAL TOKENS"),
                        )
                        .child(div().text_2xl().font_bold().child(fmt_tokens(total_tokens))),
                ),
        )
        .child(provider_bars)
}
