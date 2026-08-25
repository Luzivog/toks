use gpui::{div, prelude::*};
use gpui_component::ActiveTheme;

use crate::ToksApp;

pub(super) fn banked_reset_error(app: &ToksApp, cx: &gpui::App) -> Option<gpui::Div> {
    let error = app.banked_resets.error()?.to_string();
    Some(
        div()
            .debug_selector(|| "rotation-reset-error".to_string())
            .px_4()
            .py_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_sm()
            .text_color(cx.theme().danger)
            .child(error),
    )
}
