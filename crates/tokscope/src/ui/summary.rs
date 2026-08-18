use gpui::{div, prelude::*, px, App};
use gpui_component::{h_flex, progress::Progress, v_flex, ActiveTheme, StyledExt};

use super::{chart_tooltip::ProviderPoint, claude_accent, codex_accent, fmt_cost_full, fmt_tokens};

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct UsageSummary {
    pub(super) claude_cost: f64,
    pub(super) claude_tokens: i64,
    pub(super) codex_cost: f64,
    pub(super) codex_tokens: i64,
}

impl UsageSummary {
    pub(super) fn from_points(data: &[ProviderPoint]) -> Self {
        data.iter().fold(Self::default(), |mut summary, point| {
            summary.claude_cost += point.claude;
            summary.claude_tokens = summary.claude_tokens.saturating_add(point.claude_tokens);
            summary.codex_cost += point.codex;
            summary.codex_tokens = summary.codex_tokens.saturating_add(point.codex_tokens);
            summary
        })
    }

    pub(super) fn exact(
        claude_cost: f64,
        claude_tokens: i64,
        codex_cost: f64,
        codex_tokens: i64,
    ) -> Self {
        Self {
            claude_cost: claude_cost.max(0.0),
            claude_tokens: claude_tokens.max(0),
            codex_cost: codex_cost.max(0.0),
            codex_tokens: codex_tokens.max(0),
        }
    }
}

pub(super) fn usage_summary_sidebar(
    summary: UsageSummary,
    eyebrow: &'static str,
    cx: &App,
) -> gpui::Div {
    let UsageSummary {
        claude_cost,
        claude_tokens,
        codex_cost,
        codex_tokens,
    } = summary;
    let total_cost = claude_cost + codex_cost;
    let total_tokens = claude_tokens.saturating_add(codex_tokens);
    let cost = fmt_cost_full(total_cost);
    let mut providers = vec![
        ("Codex", codex_accent(), codex_cost, codex_tokens),
        ("Claude Code", claude_accent(), claude_cost, claude_tokens),
    ];
    providers.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));

    let mut provider_bars = v_flex().gap_5();
    for (name, color, cost, tokens) in providers {
        let share = if total_cost > 0.0 {
            cost / total_cost * 100.0
        } else if total_tokens > 0 {
            tokens as f64 / total_tokens as f64 * 100.0
        } else {
            0.0
        };
        provider_bars = provider_bars.child(
            v_flex()
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
