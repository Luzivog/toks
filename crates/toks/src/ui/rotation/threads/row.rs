use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::{
    rotation::{ThreadRow, ThreadStatus},
    Provider,
};

use crate::{Page, ToksApp};

use super::{presentation, selectors};

const STATUS_WIDTH: f32 = 150.;

pub(super) fn thread_row(
    app: &ToksApp,
    row: &ThreadRow,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let thread_id = row.thread_id.as_str();
    let row_selector = format!("rotation-thread-row-{thread_id}");
    let title_selector = format!("rotation-thread-title-{thread_id}");
    let status_selector = format!("rotation-thread-status-{thread_id}");
    let status_dot_selector = format!("rotation-thread-status-dot-{thread_id}");
    let title = presentation::thread_title(&app.rotation.thread_titles, &row.thread_id);
    let show_id = title != thread_id;
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
                .child(
                    div()
                        .when(show_id, |title| {
                            title.debug_selector(move || title_selector.clone())
                        })
                        .text_sm()
                        .font_medium()
                        .truncate()
                        .child(title),
                )
                .when(show_id || account.is_some(), |column| {
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
                            }),
                    )
                }),
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
        .child(selectors::selectors(app, row, cx))
}
