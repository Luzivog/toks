use gpui::{div, prelude::*, px, App};
use gpui_component::{box_shadow, ActiveTheme};

pub(super) fn account_email(
    email: &str,
    hidden: bool,
    provider: &str,
    account_id: &str,
    cx: &App,
) -> gpui::Div {
    let selector = format!("account-email-{provider}-{account_id}");
    let blur_selector = format!("account-email-blur-{provider}-{account_id}");
    div()
        .debug_selector(move || selector.clone())
        .relative()
        .min_w_0()
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_xs()
                .text_color(
                    cx.theme()
                        .muted_foreground
                        .opacity(if hidden { 0.0 } else { 1.0 }),
                )
                .child(email.to_string()),
        )
        .when(hidden, |email| email.child(privacy_blur(blur_selector, cx)))
}

fn privacy_blur(selector: String, cx: &App) -> gpui::Div {
    let haze = cx.theme().muted_foreground.opacity(0.16);
    let inner_glow = cx.theme().foreground.opacity(0.07);
    div()
        .debug_selector(move || selector.clone())
        .absolute()
        .top_0()
        .bottom_0()
        .left_0()
        .right_0()
        .overflow_hidden()
        .rounded_md()
        .bg(cx.theme().secondary.opacity(0.72))
        .child(
            div()
                .absolute()
                .top(px(2.))
                .bottom(px(2.))
                .left(px(3.))
                .right(px(3.))
                .rounded_md()
                .bg(haze)
                .shadow(vec![
                    box_shadow(px(0.), px(0.), px(6.), px(1.), haze),
                    box_shadow(px(0.), px(0.), px(12.), px(2.), inner_glow),
                ]),
        )
}
