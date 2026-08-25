use gpui::{div, prelude::*};
use gpui_component::{
    checkbox::Checkbox, h_flex, v_flex, ActiveTheme, Disableable, Sizable, StyledExt,
};
use toks_core::{ClientId, USAGE_PROVIDERS};

use crate::ToksApp;

use super::{accent_for_usage_provider, section_title, usage_provider_label};

pub(super) fn settings_page(app: &ToksApp, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let mut providers = v_flex()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            v_flex()
                .gap_1()
                .p_4()
                .child(section_title("Providers"))
                .child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("Choose which providers appear in usage views."),
                ),
        );
    for provider in USAGE_PROVIDERS {
        providers = providers.child(provider_row(app, provider, cx));
    }
    providers = providers.child(
        div()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_xs()
            .text_color(cx.theme().muted_foreground)
            .child("At least one provider must remain visible."),
    );
    if let Some(error) = &app.provider_visibility_error {
        providers = providers.child(
            div()
                .px_4()
                .pb_3()
                .text_xs()
                .text_color(cx.theme().danger)
                .child(error.clone()),
        );
    }

    v_flex()
        .debug_selector(|| "settings-page".to_string())
        .p_6()
        .gap_6()
        .child(div().text_2xl().font_bold().child("Settings"))
        .child(providers)
}

fn provider_row(app: &ToksApp, provider: ClientId, cx: &mut gpui::Context<ToksApp>) -> gpui::Div {
    let checked = app.provider_visibility.is_visible(provider);
    let disabled = checked && !app.provider_visibility.can_hide(provider);
    let handle = cx.entity().downgrade();
    div()
        .debug_selector(move || format!("settings-provider-{}", provider.as_str()))
        .px_4()
        .py_3()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            Checkbox::new(("settings-provider-checkbox", provider as usize))
                .small()
                .checked(checked)
                .disabled(disabled)
                .on_click(move |visible, _, cx| {
                    let _ = handle.update(cx, |app, cx| {
                        app.set_provider_visible(provider, *visible);
                        cx.notify();
                    });
                })
                .child(
                    h_flex()
                        .gap_2()
                        .items_center()
                        .child(
                            div()
                                .size_2()
                                .rounded_full()
                                .bg(accent_for_usage_provider(provider)),
                        )
                        .child(usage_provider_label(provider)),
                ),
        )
}
