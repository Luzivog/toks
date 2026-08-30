use gpui::{div, prelude::*, px, SharedString};
use gpui_component::{h_flex, tooltip::Tooltip, ActiveTheme, StyledExt};
use toks_core::{accounts::AccountId, LimitSnapshot};

use super::state::{general_weekly_window, AccountState};

pub(super) fn weekly_meter(snapshot: &LimitSnapshot, cx: &gpui::App) -> Option<gpui::Div> {
    general_weekly_window(snapshot).map(|window| {
        let selector = format!("rotation-account-meter-{}", snapshot.account.id);
        let remaining = window.percent_remaining();
        let fill =
            super::super::super::gauge_color(window, cx.theme().muted_foreground.opacity(0.55), cx);
        h_flex()
            .gap_2()
            .items_center()
            .child(
                super::super::super::quota_progress(remaining, fill, cx)
                    .debug_selector(move || selector.clone())
                    .w(px(72.))
                    .h(px(3.)),
            )
            .child(
                div()
                    .w(px(34.))
                    .flex_shrink_0()
                    .text_right()
                    .text_xs()
                    .font_medium()
                    .text_color(cx.theme().muted_foreground)
                    .child(format!("{remaining:.0}%")),
            )
    })
}

pub(super) fn account_status(
    id: &AccountId,
    state: AccountState,
    meter: Option<gpui::Div>,
    active: Option<u32>,
    cx: &gpui::App,
) -> gpui::Div {
    let status_selector = format!("rotation-account-status-{id}");
    let tooltip_selector = format!("rotation-account-status-tooltip-{id}");
    let status_label = match active {
        Some(active) if active > 0 => format!("{} · {active} active", state.label),
        Some(_) => state.label,
        None => state.label,
    };
    let status_text = div()
        .id(SharedString::from(status_selector.clone()))
        .debug_selector(move || status_selector.clone())
        .flex_1()
        .min_w_0()
        .truncate()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .child(status_label)
        .when_some(
            state.reset_at.map(super::super::super::fmt_exact_local),
            |status, exact| {
                status.tooltip(move |window, cx| {
                    let exact = exact.clone();
                    let selector = tooltip_selector.clone();
                    Tooltip::element(move |_, _| {
                        let selector = selector.clone();
                        div()
                            .debug_selector(move || selector.clone())
                            .child(exact.clone())
                    })
                    .build(window, cx)
                })
            },
        );
    let metadata = h_flex()
        .ml_auto()
        .items_center()
        .when_some(meter, |metadata, meter| metadata.child(meter));

    h_flex()
        .gap_1p5()
        .items_center()
        .child(
            div()
                .size(px(6.))
                .flex_shrink_0()
                .rounded_full()
                .bg(state.color),
        )
        .child(status_text)
        .child(metadata)
}
