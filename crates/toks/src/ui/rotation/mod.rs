use gpui::{div, prelude::*, px};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};
use toks_core::Provider;

use crate::ToksApp;

mod accounts;
mod events;
mod format;
mod status;
mod waiting;

pub(super) fn rotation_page(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let has_emails = app
        .limits
        .iter()
        .any(|snapshot| snapshot.provider == Provider::Codex && snapshot.account.email.is_some());
    let email_toggle = has_emails.then(|| {
        super::account_email::email_visibility_button(
            "rotation-toggle-account-emails",
            app.emails_hidden,
            cx,
        )
    });
    let mut page = v_flex()
        .debug_selector(|| "rotation-page".into())
        .w_full()
        .min_w_0()
        .p_6()
        .gap_5()
        .child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .gap_2()
                        .child(div().text_2xl().font_bold().child("Codex rotation"))
                        .when_some(email_toggle, |title, toggle| title.child(toggle)),
                )
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Route new local Codex work through your available subscriptions."),
                ),
        )
        .child(status::service_card(app, cx))
        .child(accounts::accounts_card(app, cx))
        .child(waiting::waiting_card(app, cx))
        .child(events::events_card(app, cx));

    if let Some(error) = &app.rotation.error {
        page = page.child(
            div()
                .debug_selector(|| "rotation-error".into())
                .px_4()
                .py_3()
                .rounded_lg()
                .border_1()
                .border_color(cx.theme().danger.opacity(0.55))
                .bg(cx.theme().danger.opacity(0.1))
                .text_sm()
                .flex()
                .items_center()
                .justify_between()
                .child(div().child(error.clone()))
                .child(
                    super::text_action("rotation-dismiss-error", "Dismiss", cx).on_click(
                        cx.listener(|app, _, _, cx| {
                            app.rotation.error = None;
                            cx.notify();
                        }),
                    ),
                ),
        );
    }
    page
}

fn card(title: &'static str, meta: String, cx: &gpui::App) -> gpui::Div {
    v_flex()
        .w_full()
        .overflow_hidden()
        .rounded_xl()
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .child(
            div()
                .h(px(42.))
                .px_4()
                .flex()
                .items_center()
                .justify_between()
                .child(div().text_sm().font_semibold().child(title))
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(meta),
                ),
        )
}

fn empty_row(message: &'static str, cx: &gpui::App) -> gpui::Div {
    div()
        .px_4()
        .py_4()
        .border_t_1()
        .border_color(cx.theme().border)
        .text_sm()
        .text_color(cx.theme().muted_foreground)
        .child(message)
}
