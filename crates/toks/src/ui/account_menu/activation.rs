use std::rc::Rc;

use gpui::{App, Window};
use gpui_component::menu::PopupMenuItem;
use toks_core::{
    accounts::AccountId,
    codex_router::account_activation::{
        AccountActivationStatus, AutomaticTestStatus, ManualTestStatus,
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
    if status.automatic == AutomaticTestStatus::NeedsAttention {
        items.push(PopupMenuItem::new("Automatic test needs attention").disabled(true));
    }
    items.push(automatic);
    items
}

fn test_action(status: &AccountActivationStatus) -> (&'static str, bool) {
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

#[cfg(test)]
mod tests {
    use super::test_action;
    use toks_core::codex_router::account_activation::{
        AccountActivationStatus, AutomaticTestStatus, ManualTestStatus,
    };

    #[test]
    fn account_test_action_is_state_specific() {
        let mut status = AccountActivationStatus::default();
        assert_eq!(test_action(&status), ("Send test", false));
        status.manual = ManualTestStatus::Running;
        assert_eq!(test_action(&status), ("Sending test…", true));
        status.manual = ManualTestStatus::Failed;
        assert_eq!(test_action(&status), ("Retry test", false));
        status.automatic = AutomaticTestStatus::Pending;
        assert_eq!(test_action(&status), ("Sending test…", true));
        status.automatic = AutomaticTestStatus::NeedsAttention;
        assert_eq!(test_action(&status), ("Retry weekly reset test", false));
    }
}
