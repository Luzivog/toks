use gpui::{div, prelude::*, ClipboardItem};
use gpui_component::{h_flex, v_flex, ActiveTheme, StyledExt};

use crate::ToksApp;

pub(super) fn pairing_panel(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> Option<gpui::Div> {
    let pairing = app.rotation.remote.pairing.as_ref()?;
    let seconds = (pairing.expires_at - app.now.timestamp()).max(0);
    let expiry = format!("Expires in {}m {}s", seconds / 60, seconds % 60);
    let code = pairing.manual_code.clone();
    Some(
        v_flex()
            .debug_selector(|| "rotation-remote-pairing-panel".into())
            .gap_2()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .child(
                h_flex()
                    .justify_between()
                    .child(div().text_sm().font_medium().child("Add a device"))
                    .child(
                        div()
                            .debug_selector(|| "rotation-remote-pairing-expires".into())
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(expiry),
                    ),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child("On your phone, open Codex settings, choose Connections, then enter:"),
            )
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .debug_selector(|| "rotation-remote-pairing-code".into())
                            .text_lg()
                            .font_semibold()
                            .child(code.clone()),
                    )
                    .child(
                        h_flex()
                            .gap_1()
                            .child(
                                super::super::super::text_action(
                                    "rotation-remote-copy-code",
                                    "Copy code",
                                    cx,
                                )
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(code.clone()));
                                }),
                            )
                            .child(
                                super::super::super::text_action(
                                    "rotation-remote-cancel-pairing",
                                    "Cancel",
                                    cx,
                                )
                                .on_click(cx.listener(
                                    |app, _, _, cx| {
                                        app.rotation.remote.pairing = None;
                                        app.rotation.remote.panel = super::RemotePanel::Summary;
                                        cx.notify();
                                    },
                                )),
                            ),
                    ),
            ),
    )
}
