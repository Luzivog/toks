use gpui::{div, prelude::*};
use gpui_component::{menu::PopupMenuItem, ActiveTheme};
use toks_core::rotation::{ThreadId, ThreadOverrideChange};

use crate::{app::SettingsAction, ToksApp};

use super::super::choices::Choice;
use super::kind::SelectorKind;

pub(super) const CLEAR_OVERRIDE_LABEL: &str = "Clear override";

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum SelectorMenuEntry {
    Choice { choice: Choice, checked: bool },
    Separator,
    ClearOverride,
}

pub(super) fn entries(
    kind: SelectorKind,
    mut choices: Vec<Choice>,
    thread_override: Option<&str>,
    observed: Option<&str>,
) -> Vec<SelectorMenuEntry> {
    let effective = thread_override.or(observed);
    if let Some(value) = effective {
        if !choices.iter().any(|choice| choice.value == value) {
            choices.push(Choice {
                value: value.to_owned(),
                label: kind.display(value),
            });
        }
    }

    let mut entries = choices
        .into_iter()
        .map(|choice| {
            let checked = effective == Some(choice.value.as_str());
            SelectorMenuEntry::Choice { choice, checked }
        })
        .collect::<Vec<_>>();
    if thread_override.is_some() {
        entries.push(SelectorMenuEntry::Separator);
        entries.push(SelectorMenuEntry::ClearOverride);
    }
    entries
}

pub(super) fn choice_item(
    choice: Choice,
    checked: bool,
    thread: ThreadId,
    kind: SelectorKind,
    app: gpui::WeakEntity<ToksApp>,
) -> PopupMenuItem {
    let change = kind.change(Some(choice.value));
    on_change(
        PopupMenuItem::new(choice.label).checked(checked),
        thread,
        change,
        app,
    )
}

pub(super) fn clear_override_item(
    thread: ThreadId,
    kind: SelectorKind,
    app: gpui::WeakEntity<ToksApp>,
) -> PopupMenuItem {
    let selector = format!(
        "rotation-thread-{}-{}-clear-override",
        kind.slug(),
        thread.as_str()
    );
    let item = PopupMenuItem::element(move |_, cx| {
        div()
            .debug_selector({
                let selector = selector.clone();
                move || selector.clone()
            })
            .size_full()
            .flex()
            .items_center()
            .cursor_pointer()
            .text_color(cx.theme().muted_foreground)
            .child(CLEAR_OVERRIDE_LABEL)
    });
    on_change(item, thread, kind.change(None), app)
}

fn on_change(
    item: PopupMenuItem,
    thread: ThreadId,
    change: ThreadOverrideChange,
    app: gpui::WeakEntity<ToksApp>,
) -> PopupMenuItem {
    item.on_click(move |_, _, cx| {
        let thread = thread.clone();
        let change = change.clone();
        let _ = app.update(cx, |app, cx| {
            app.change_rotation_settings(SettingsAction::SetThreadOverride(thread, change), cx);
        });
    })
}
