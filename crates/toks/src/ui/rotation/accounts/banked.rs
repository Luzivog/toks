use gpui::{div, prelude::*};
use gpui_component::{h_flex, ActiveTheme};
use toks_core::{rotation::UnixMillis, Provider};

use crate::ToksApp;

pub(super) fn banked_reset_note(app: &ToksApp, cx: &gpui::App) -> Option<gpui::Div> {
    let included: Vec<_> = app
        .rotation
        .settings
        .priority()
        .iter()
        .filter(|account| !app.rotation.settings.excluded().contains(*account))
        .collect();
    let now = UnixMillis::new(app.now.timestamp_millis());
    if included.is_empty()
        || included
            .iter()
            .any(|account| app.rotation.runtime.is_available(account, now))
    {
        return None;
    }
    let resets: u64 = app
        .limits
        .iter()
        .filter(|snapshot| {
            snapshot.provider == Provider::Codex && included.contains(&&snapshot.account.id)
        })
        .map(|snapshot| snapshot.banked_resets)
        .sum();
    if resets == 0 {
        return None;
    }
    let noun = if resets == 1 {
        "reset is"
    } else {
        "resets are"
    };
    Some(
        div()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .bg(gpui::rgba(0x10_a3_7f_1a))
            .text_sm()
            .child(format!(
                "{resets} banked {noun} available. Use reset on an unavailable account to reset its 5-hour and weekly limits. Toks never uses resets automatically."
            )),
    )
}

pub(super) fn banked_reset_result(
    app: &ToksApp,
    cx: &mut gpui::Context<ToksApp>,
) -> Option<gpui::Div> {
    let notice = app.banked_resets.notice()?.to_string();
    Some(
        h_flex()
            .px_4()
            .py_2()
            .gap_2()
            .border_t_1()
            .border_color(cx.theme().border)
            .justify_between()
            .child(div().min_w_0().text_sm().child(notice))
            .child(
                super::super::super::text_action("rotation-dismiss-reset-notice", "Dismiss", cx)
                    .compact()
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.banked_resets.dismiss_notice();
                        cx.notify();
                    })),
            ),
    )
}
