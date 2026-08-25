use gpui::{div, prelude::*, px, Corner};
use gpui_component::{
    h_flex,
    menu::{DropdownMenu, PopupMenuItem},
    ActiveTheme, Disableable,
};
use toks_core::rotation::{ThreadId, ThreadOverrideChange, ThreadRow};

use crate::{app::SettingsAction, ToksApp};

use super::{
    choices::{self, Choice},
    presentation::{selector_label, SelectorSource},
};

mod kind;
use kind::SelectorKind;

struct SelectorSpec {
    kind: SelectorKind,
    thread_override: Option<String>,
    observed: Option<String>,
    choices: Vec<Choice>,
}

pub(super) fn selectors(
    app: &ToksApp,
    row: &ThreadRow,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let thread_override = app.rotation.settings.thread_override(&row.thread_id);
    let model_override = thread_override
        .and_then(|value| value.model())
        .map(str::to_owned);
    let reasoning_override = thread_override
        .and_then(|value| value.reasoning_effort())
        .map(str::to_owned);
    let tier_override = thread_override
        .and_then(|value| value.service_tier())
        .map(str::to_owned);
    let effective_model = model_override
        .as_deref()
        .or(row.request_settings.model.as_deref());
    let model_choices = choices::models(
        &app.rotation.selectable_models,
        row.request_settings.model.as_deref(),
    );
    let reasoning_choices = choices::reasoning(&app.rotation.selectable_models, effective_model);

    h_flex()
        .flex_shrink_0()
        .gap_1()
        .child(selector(
            app,
            &row.thread_id,
            SelectorSpec {
                kind: SelectorKind::Model,
                thread_override: model_override,
                observed: row.request_settings.model.clone(),
                choices: model_choices,
            },
            cx,
        ))
        .child(selector(
            app,
            &row.thread_id,
            SelectorSpec {
                kind: SelectorKind::Reasoning,
                thread_override: reasoning_override,
                observed: row.request_settings.reasoning_effort.clone(),
                choices: reasoning_choices,
            },
            cx,
        ))
        .child(selector(
            app,
            &row.thread_id,
            SelectorSpec {
                kind: SelectorKind::Tier,
                thread_override: tier_override,
                observed: row.request_settings.service_tier.clone(),
                choices: choices::tiers(),
            },
            cx,
        ))
}

fn selector(
    app: &ToksApp,
    thread: &ThreadId,
    spec: SelectorSpec,
    cx: &mut gpui::Context<ToksApp>,
) -> impl IntoElement {
    let SelectorSpec {
        kind,
        thread_override,
        observed,
        choices,
    } = spec;
    let label = selector_label(thread_override.as_deref(), observed.as_deref());
    let value_color = if label.source == SelectorSource::Override {
        cx.theme().foreground
    } else {
        cx.theme().muted_foreground
    };
    let handle = cx.entity().downgrade();
    let menu_thread = thread.clone();
    let current = thread_override.clone();
    super::super::super::action_button(
        format!("rotation-thread-{}-{}", kind.slug(), thread.as_str()),
        cx,
    )
    .compact()
    .w(px(kind.width()))
    .h(px(26.))
    .overflow_hidden()
    .text_xs()
    .dropdown_caret(true)
    .disabled(app.rotation.busy.is_some())
    .tooltip(format!("{} for the next request", kind.label()))
    .child(
        h_flex()
            .min_w_0()
            .gap_1()
            .child(
                div()
                    .flex_shrink_0()
                    .text_color(cx.theme().muted_foreground)
                    .child(kind.label()),
            )
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_color(value_color)
                    .child(kind.display(&label.text)),
            ),
    )
    .dropdown_menu_with_anchor(Corner::TopRight, move |mut menu, _, _| {
        menu = menu.min_w(px(160.)).item(menu_item(
            "Auto".into(),
            current.is_none(),
            menu_thread.clone(),
            kind.change(None),
            handle.clone(),
        ));
        for choice in &choices {
            menu = menu.item(menu_item(
                choice.label.clone(),
                current.as_deref() == Some(choice.value.as_str()),
                menu_thread.clone(),
                kind.change(Some(choice.value.clone())),
                handle.clone(),
            ));
        }
        menu
    })
}

fn menu_item(
    label: String,
    checked: bool,
    thread: ThreadId,
    change: ThreadOverrideChange,
    app: gpui::WeakEntity<ToksApp>,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .checked(checked)
        .on_click(move |_, _, cx| {
            let thread = thread.clone();
            let change = change.clone();
            let _ = app.update(cx, |app, cx| {
                app.change_rotation_settings(SettingsAction::SetThreadOverride(thread, change), cx);
            });
        })
}
