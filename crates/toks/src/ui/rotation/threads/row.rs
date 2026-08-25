use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, Disableable, StyledExt};
use toks_core::{
    rotation::{ThreadRow, ThreadStatus},
    Provider,
};

use crate::{app::SettingsAction, Page, ToksApp};

use super::{grouping::DisplayThread, presentation, selectors};

const INDENT_WIDTH: f32 = 14.;
const MAX_VISUAL_DEPTH: usize = 3;
const STATUS_WIDTH: f32 = 150.;
const ACTION_WIDTH: f32 = 64.;

pub(super) fn thread_row(
    app: &ToksApp,
    display: &DisplayThread<'_>,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let row: &ThreadRow = display.row;
    let thread_id = row.thread_id.as_str();
    let row_selector = format!("rotation-thread-row-{thread_id}");
    let title_selector = format!("rotation-thread-title-{thread_id}");
    let status_selector = format!("rotation-thread-status-{thread_id}");
    let status_dot_selector = format!("rotation-thread-status-dot-{thread_id}");
    let title = presentation::thread_title(
        &app.rotation.thread_titles,
        app.rotation.thread_lineage.get(&row.thread_id),
        &row.thread_id,
    );
    let show_id = title != thread_id;
    let indicator = display.indicator.as_deref();
    let status = match row.last_activity_at {
        Some(at) => format!(
            "{} · {}",
            presentation::status_label(row.status),
            super::super::format::age(app.now, at)
        ),
        None => presentation::status_label(row.status).to_owned(),
    };
    let status_color = if matches!(row.status, ThreadStatus::Streaming { .. }) {
        super::super::super::page_accent(Page::Rotation, cx)
    } else {
        cx.theme().muted_foreground
    };
    let account = row.account_id.as_ref().filter(|account| {
        app.limits.iter().any(|snapshot| {
            snapshot.provider == Provider::Codex && &snapshot.account.id == *account
        })
    });
    let account_surface = format!("rotation-thread-{thread_id}");

    h_flex()
        .debug_selector(move || row_selector.clone())
        .min_h(px(50.))
        .gap_3()
        .px_4()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap_0p5()
                .pl(px(INDENT_WIDTH * display.depth.min(MAX_VISUAL_DEPTH) as f32))
                .child(
                    h_flex()
                        .min_w_0()
                        .gap_1p5()
                        .when(display.depth > 0, |title| {
                            title.child(
                                div()
                                    .flex_shrink_0()
                                    .text_sm()
                                    .text_color(cx.theme().muted_foreground)
                                    .child("└"),
                            )
                        })
                        .child(
                            div()
                                .when(show_id, |title| {
                                    title.debug_selector(move || title_selector.clone())
                                })
                                .min_w_0()
                                .text_sm()
                                .font_medium()
                                .truncate()
                                .child(title),
                        ),
                )
                .when(
                    show_id || account.is_some() || indicator.is_some(),
                    |column| {
                        column.child(
                            h_flex()
                                .min_w_0()
                                .gap_1()
                                .text_xs()
                                .text_color(cx.theme().muted_foreground)
                                .when(show_id, |details| {
                                    details.child(div().truncate().child(thread_id.to_owned()))
                                })
                                .when(show_id && account.is_some(), |details| details.child("·"))
                                .when_some(account, |details, account| {
                                    details.child(super::super::format::account_identity(
                                        app,
                                        account,
                                        &account_surface,
                                        div().truncate(),
                                        cx,
                                    ))
                                })
                                .when(
                                    indicator.is_some() && (show_id || account.is_some()),
                                    |details| details.child("·"),
                                )
                                .when_some(indicator, |details, indicator| {
                                    details.child(div().truncate().child(indicator.to_owned()))
                                }),
                        )
                    },
                ),
        )
        .child(
            h_flex()
                .debug_selector(move || status_selector.clone())
                .w(px(STATUS_WIDTH))
                .flex_shrink_0()
                .min_w_0()
                .items_center()
                .justify_end()
                .gap_1p5()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(
                    div()
                        .debug_selector(move || status_dot_selector.clone())
                        .size(px(6.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(status_color),
                )
                .child(div().min_w_0().truncate().child(status)),
        )
        .child(dismiss_action(app, row, cx))
        .child(selectors::selectors(app, row, cx))
}

fn dismiss_action(app: &ToksApp, row: &ThreadRow, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let thread = row.thread_id.clone();
    let pending = app.rotation.settings.cancelled_threads().contains(&thread);
    div()
        .w(px(ACTION_WIDTH))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_end()
        .when(
            matches!(row.status, ThreadStatus::AwaitingFollowUp),
            |slot| {
                slot.child(
                    super::super::super::text_action(
                        format!("rotation-dismiss-thread-{}", thread.as_str()),
                        "Dismiss",
                        cx,
                    )
                    .compact()
                    .disabled(app.rotation.busy.is_some() || pending)
                    .tooltip("Remove this dormant thread from Toks without deleting it in Codex")
                    .on_click(cx.listener(move |app, _, _, cx| {
                        app.change_rotation_settings(SettingsAction::Cancel(thread.clone()), cx);
                    })),
                )
            },
        )
}
