use chrono::{DateTime, Utc};
use gpui::{div, prelude::*, px, relative, App, Hsla, SharedString};
use gpui_component::{h_flex, tooltip::Tooltip, ActiveTheme, StyledExt};
use toks_core::limits::LimitWindow;

use super::{fmt_exact_local, fmt_reset, gauge_color};

pub(super) fn quota_row(
    window: &LimitWindow,
    now: DateTime<Utc>,
    accent: Hsla,
    cx: &App,
) -> gpui::Div {
    let elapsed = window.reset_elapsed(now);
    let remaining = window.percent_remaining();
    let color = if elapsed {
        cx.theme().muted_foreground
    } else {
        gauge_color(window, accent, cx)
    };
    let value = if elapsed {
        format!("Last known {remaining:.0}%")
    } else {
        format!("{remaining:.0}% left")
    };
    let value_color = if elapsed || remaining > 20.0 {
        cx.theme().foreground
    } else {
        color
    };
    let (label, scope) = split_limit_label(&window.label);
    let selector = format!("quota-row-{}", window.id);
    let reset_selector = format!("quota-reset-{}", window.id);
    let reset_tooltip_selector = format!("quota-reset-tooltip-{}", window.id);
    let reset_exact = window.resets_at.map(fmt_exact_local);

    h_flex()
        .debug_selector(move || selector.clone())
        .w_full()
        .h(px(46.))
        .gap_3()
        .px_4()
        .border_t_1()
        .border_color(cx.theme().border.opacity(0.72))
        .child(
            h_flex()
                .w(px(230.))
                .flex_shrink_0()
                .min_w_0()
                .gap_2()
                .child(div().text_sm().font_medium().child(label.to_string()))
                .when_some(scope, |row, scope| {
                    row.child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(scope.to_string()),
                    )
                }),
        )
        .child(quota_progress(remaining, color, cx).flex_1())
        .child(
            h_flex()
                .w(px(230.))
                .flex_shrink_0()
                .justify_end()
                .gap_2()
                .child(
                    div()
                        .w(px(76.))
                        .flex_shrink_0()
                        .text_right()
                        .text_sm()
                        .font_semibold()
                        .text_color(value_color)
                        .child(value),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child("·"),
                )
                .child(
                    div()
                        .id(SharedString::from(reset_selector.clone()))
                        .debug_selector(move || reset_selector.clone())
                        .w(px(132.))
                        .flex_shrink_0()
                        .text_right()
                        .truncate()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(fmt_reset(now, window.resets_at))
                        .when_some(reset_exact, |reset, exact| {
                            reset.tooltip(move |window, cx| {
                                let exact = exact.clone();
                                let selector = reset_tooltip_selector.clone();
                                Tooltip::element(move |_, _| {
                                    let selector = selector.clone();
                                    div()
                                        .debug_selector(move || selector.clone())
                                        .child(exact.clone())
                                })
                                .build(window, cx)
                            })
                        }),
                ),
        )
}

fn quota_progress(remaining: f64, color: Hsla, cx: &App) -> gpui::Div {
    div()
        .relative()
        .w_full()
        .h(px(6.))
        .overflow_hidden()
        .rounded_full()
        .bg(cx.theme().foreground.opacity(0.13))
        .when(remaining > 0.0, |track| {
            track.child(
                div()
                    .absolute()
                    .left_0()
                    .top_0()
                    .h_full()
                    .min_w(px(2.))
                    .w(relative((remaining / 100.0) as f32))
                    .rounded_full()
                    .bg(color),
            )
        })
}

pub(super) fn split_limit_label(label: &str) -> (&str, Option<&str>) {
    if let Some((name, scope)) = label.split_once(" — ") {
        return (name, Some(scope));
    }
    if let Some(name) = label.strip_suffix(" (all models)") {
        return (name, Some("All models"));
    }
    (label, None)
}
