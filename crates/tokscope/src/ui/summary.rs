use gpui::{div, prelude::*, px, App};
use gpui_component::{h_flex, progress::Progress, v_flex, ActiveTheme, StyledExt};

use super::{chart_tooltip::ProviderPoint, claude_accent, codex_accent, fmt_cost_full, fmt_tokens};

pub(super) fn usage_summary_sidebar(
    data: &[ProviderPoint],
    eyebrow: &'static str,
    cx: &App,
) -> gpui::Div {
    let claude_cost: f64 = data.iter().map(|point| point.claude).sum();
    let codex_cost: f64 = data.iter().map(|point| point.codex).sum();
    let claude_tokens = data.iter().fold(0_i64, |total, point| {
        total.saturating_add(point.claude_tokens)
    });
    let codex_tokens = data.iter().fold(0_i64, |total, point| {
        total.saturating_add(point.codex_tokens)
    });
    let total_cost = claude_cost + codex_cost;
    let total_tokens = data.iter().fold(0_i64, |total, point| {
        total
            .saturating_add(point.claude_tokens)
            .saturating_add(point.codex_tokens)
    });
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
