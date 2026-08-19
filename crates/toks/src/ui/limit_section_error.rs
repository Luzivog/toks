use gpui::{div, prelude::*, Context};
use gpui_component::{h_flex, ActiveTheme};

use crate::ToksApp;

use super::text_action;

pub(super) fn account_error_rows(app: &ToksApp, cx: &mut Context<ToksApp>) -> Vec<gpui::Div> {
    app.account_operations
        .errors()
        .iter()
        .cloned()
        .map(|error| {
            let dismiss = text_action(format!("dismiss-account-error-{}", error.id), "Dismiss", cx)
                .compact()
                .on_click(cx.listener(move |app, _, _, cx| {
                    app.account_operations.dismiss_error(error.id);
                    cx.notify();
                }));
            h_flex()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .justify_between()
                .gap_3()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(div().min_w_0().child(error.message))
                .child(dismiss)
        })
        .collect()
}
