use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, StyledExt};
use toks_core::rotation::{ThreadId, WaitingThread};

use crate::{app::SettingsAction, ToksApp};

use super::{card, empty_row};

pub(super) fn waiting_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let waiting: Vec<_> = app
        .rotation
        .settings
        .waiting_priority()
        .iter()
        .filter_map(|thread| {
            app.rotation
                .runtime
                .waiting_threads()
                .iter()
                .find(|waiting| &waiting.thread_id == thread)
        })
        .collect();
    let mut panel = card("Waiting threads", waiting.len().to_string(), cx);
    if waiting.is_empty() {
        return panel.child(empty_row("No threads are waiting for an account.", cx));
    }
    for (index, thread) in waiting.iter().enumerate() {
        panel = panel.child(waiting_row(app, thread, index, waiting.len(), cx));
    }
    panel
}

fn waiting_row(
    app: &ToksApp,
    waiting: &WaitingThread,
    index: usize,
    count: usize,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let thread = waiting.thread_id.clone();
    let busy = app.rotation.busy.is_some();
    h_flex()
        .min_h(px(50.))
        .gap_3()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .w(px(22.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(format!("{}", index + 1)),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .child(
                    div()
                        .text_sm()
                        .font_medium()
                        .truncate()
                        .child(thread.as_str().to_owned()),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(format!(
                            "Waiting {}",
                            super::format::age(app.now, waiting.since)
                        )),
                ),
        )
        .child(
            h_flex()
                .gap_1()
                .child(move_button(
                    "up",
                    "↑",
                    &thread,
                    index.saturating_sub(1),
                    index == 0 || busy,
                    cx,
                ))
                .child(move_button(
                    "down",
                    "↓",
                    &thread,
                    index + 1,
                    index + 1 >= count || busy,
                    cx,
                )),
        )
        .child(
            super::super::text_action(
                format!("rotation-cancel-thread-{}", thread.as_str()),
                "Cancel",
                cx,
            )
            .disabled(busy)
            .tooltip(format!(
                "Queued since {}",
                super::format::exact_time(waiting.since)
            ))
            .on_click(cx.listener(move |app, _, _, cx| {
                app.change_rotation_settings(SettingsAction::Cancel(thread.clone()), cx);
            })),
        )
}

fn move_button(
    direction: &'static str,
    label: &'static str,
    thread: &ThreadId,
    index: usize,
    disabled: bool,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui_component::button::Button {
    let queued = thread.clone();
    super::super::text_action(
        format!("rotation-thread-{direction}-{}", thread.as_str()),
        label,
        cx,
    )
    .compact()
    .disabled(disabled)
    .tooltip(format!("Move {direction}"))
    .on_click(cx.listener(move |app, _, _, cx| {
        app.change_rotation_settings(SettingsAction::MoveWaiting(queued.clone(), index), cx);
    }))
}
