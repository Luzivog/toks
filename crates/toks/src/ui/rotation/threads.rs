use gpui::{div, prelude::*};
use gpui_component::h_flex;

use crate::ToksApp;

use super::{card_header_annotation, card_header_title, card_with_header, empty_row};

mod choices;
mod presentation;
mod row;
mod selectors;

#[cfg(test)]
pub(super) use presentation::{
    header_count, selector_label, status_label, thread_title, visible_rows, SelectorSource,
};

pub(super) fn threads_card(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let rows = app.rotation.runtime.thread_rows();
    let header = presentation::header_count(rows.iter().map(|row| row.status));
    let title = h_flex()
        .min_w_0()
        .items_center()
        .gap_2()
        .child(
            card_header_title("Active threads")
                .debug_selector(|| "rotation-active-threads-title".into()),
        )
        .child(
            card_header_annotation(header, cx)
                .debug_selector(|| "rotation-active-threads-count".into()),
        );
    let captions = if rows.is_empty() {
        div()
    } else {
        selectors::header_captions(cx)
    };
    let mut panel = card_with_header(title, captions, cx)
        .debug_selector(|| "rotation-active-threads-card".into());
    if rows.is_empty() {
        return panel.child(empty_row("No active threads.", cx));
    }
    for thread in presentation::visible_rows(&rows) {
        panel = panel.child(row::thread_row(app, thread, cx));
    }
    panel
}
