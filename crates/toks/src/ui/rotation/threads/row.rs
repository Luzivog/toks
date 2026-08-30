use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::{rotation::ActiveTaskRow, Provider};

use crate::ToksApp;

use super::{grouping::DisplayThread, presentation, selectors};

const INDENT_WIDTH: f32 = 14.;
const MAX_VISUAL_DEPTH: usize = 3;

pub(super) fn thread_row(
    app: &ToksApp,
    display: &DisplayThread<'_>,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let row: &ActiveTaskRow = display.row;
    let thread_id = row.thread_id.as_str();
    let row_selector = format!("rotation-thread-row-{thread_id}");
    let title_selector = format!("rotation-thread-title-{thread_id}");
    let title = presentation::thread_title(
        &app.rotation.thread_titles,
        app.rotation.thread_lineage.get(&row.thread_id),
        &row.thread_id,
    );
    let show_id = title != thread_id;
    let indicator = display.indicator.as_deref();
    let account = app
        .limits
        .iter()
        .any(|snapshot| {
            snapshot.provider == Provider::Codex && snapshot.account.id == row.account_id
        })
        .then_some(&row.account_id);
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
        .child(selectors::selectors(app, row, cx))
}
