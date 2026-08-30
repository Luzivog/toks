use std::rc::Rc;

use gpui::{App, Window};
use gpui_component::menu::PopupMenuItem;
use toks_core::{
    accounts::AccountId,
    codex_router::account_activation::{
        AccountActivationStatus, AutomaticTestStatus, ManualTestOutcome, ManualTestStatus,
    },
};

#[derive(Clone)]
pub(in crate::ui) struct AccountActivationView {
    pub(in crate::ui) account: AccountId,
    pub(in crate::ui) status: Option<AccountActivationStatus>,
}

pub(in crate::ui) type AccountActivationHandler =
    Rc<dyn Fn(AccountId, &mut Window, &mut App) + 'static>;
pub(in crate::ui) type AccountActivationToggleHandler =
    Rc<dyn Fn(AccountId, bool, &mut Window, &mut App) + 'static>;

pub(super) fn items(
    view: AccountActivationView,
    on_test: AccountActivationHandler,
    on_toggle: AccountActivationToggleHandler,
) -> Vec<PopupMenuItem> {
    let Some(status) = view.status else {
        return vec![PopupMenuItem::new("Activation status unavailable").disabled(true)];
    };
    let (label, running) = test_action(&status);
    let test_account = view.account.clone();
    let test = PopupMenuItem::new(label)
        .disabled(running)
        .on_click(move |_, window, cx| on_test(test_account.clone(), window, cx));

    let automatic_account = view.account;
    let automatic_enabled = status.automatic_enabled;
    let automatic = PopupMenuItem::new("Start weekly reset automatically")
        .checked(automatic_enabled)
        .on_click(move |_, window, cx| {
            on_toggle(automatic_account.clone(), !automatic_enabled, window, cx);
        });
    let mut items = vec![test];
    if let Some(receipt) = receipt_label(&status) {
        items.push(PopupMenuItem::new(receipt).disabled(true));
    }
    if status.automatic == AutomaticTestStatus::NeedsAttention {
        items.push(PopupMenuItem::new("Automatic test needs attention").disabled(true));
    }
    items.push(automatic);
    items
}

pub(super) fn receipt_label(status: &AccountActivationStatus) -> Option<String> {
    let receipt = status.manual_receipt.as_ref()?;
    let task = receipt
        .thread_id
        .as_ref()
        .map(|thread| thread.as_str().chars().take(8).collect::<String>());
    let exact = receipt.observed_account.as_ref() == Some(&receipt.requested_account);
    let summary = match (receipt.outcome, exact) {
        (ManualTestOutcome::Succeeded, true) => "Last test succeeded on this account",
        (ManualTestOutcome::Succeeded, false) => "Last test could not verify its account",
        (ManualTestOutcome::Failed, true) => "Last test failed on this account",
        (ManualTestOutcome::Failed, false) => "Last test failed before routing",
    };
    Some(task.map_or_else(
        || summary.to_owned(),
        |task| format!("{summary} · task {task}"),
    ))
}

pub(super) fn test_action(status: &AccountActivationStatus) -> (&'static str, bool) {
    let running = matches!(
        status.manual,
        ManualTestStatus::Pending | ManualTestStatus::Running
    ) || matches!(
        status.automatic,
        AutomaticTestStatus::Pending | AutomaticTestStatus::Running
    );
    let label = if running {
        "Sending test…"
    } else if status.automatic == AutomaticTestStatus::NeedsAttention {
        "Retry weekly reset test"
    } else if status.manual == ManualTestStatus::Failed {
        "Retry test"
    } else {
        "Send test"
    };
    (label, running)
}
