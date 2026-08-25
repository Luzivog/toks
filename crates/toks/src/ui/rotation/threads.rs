use gpui::prelude::*;

use crate::ToksApp;

use super::{card, empty_row};

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
    let mut panel =
        card("Active threads", header, cx).debug_selector(|| "rotation-active-threads-card".into());
    if rows.is_empty() {
        return panel.child(empty_row("No active threads.", cx));
    }
    panel = panel.child(row::thread_captions(cx));
    for thread in presentation::visible_rows(&rows) {
        panel = panel.child(row::thread_row(app, thread, cx));
    }
    panel
}
