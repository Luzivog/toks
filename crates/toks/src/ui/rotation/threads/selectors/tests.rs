use super::{
    kind::SelectorKind,
    menu::{entries, SelectorMenuEntry, CLEAR_OVERRIDE_LABEL},
};
use crate::ui::rotation::threads::choices::Choice;

#[test]
fn menu_without_override_checks_the_observed_value_and_has_no_clear_item() {
    let entries = entries(
        SelectorKind::Model,
        vec![choice("gpt-5.6")],
        None,
        Some("gpt-5.5"),
    );

    assert_eq!(
        entries,
        vec![
            SelectorMenuEntry::Choice {
                choice: choice("gpt-5.6"),
                checked: false,
            },
            SelectorMenuEntry::Choice {
                choice: choice("gpt-5.5"),
                checked: true,
            },
        ]
    );
    assert_eq!(labels(&entries), ["gpt-5.6", "gpt-5.5"]);
}

#[test]
fn menu_with_override_checks_it_and_appends_one_clear_item() {
    let entries = entries(
        SelectorKind::Model,
        vec![choice("gpt-5.6"), choice("gpt-5.5")],
        Some("gpt-5.6"),
        Some("gpt-5.5"),
    );

    assert_eq!(
        entries,
        vec![
            SelectorMenuEntry::Choice {
                choice: choice("gpt-5.6"),
                checked: true,
            },
            SelectorMenuEntry::Choice {
                choice: choice("gpt-5.5"),
                checked: false,
            },
            SelectorMenuEntry::Separator,
            SelectorMenuEntry::ClearOverride,
        ]
    );
    assert_eq!(
        labels(&entries),
        ["gpt-5.6", "gpt-5.5", CLEAR_OVERRIDE_LABEL]
    );
}

fn choice(value: &str) -> Choice {
    Choice {
        value: value.into(),
        label: value.into(),
    }
}

fn labels(entries: &[SelectorMenuEntry]) -> Vec<&str> {
    entries
        .iter()
        .filter_map(|entry| match entry {
            SelectorMenuEntry::Choice { choice, .. } => Some(choice.label.as_str()),
            SelectorMenuEntry::Separator => None,
            SelectorMenuEntry::ClearOverride => Some(CLEAR_OVERRIDE_LABEL),
        })
        .collect()
}
