use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Corner, SharedString, Window};
use gpui_component::{
    menu::{DropdownMenu, PopupMenuItem},
    ActiveTheme, Disableable,
};
use tokscope_core::accounts::{AccountOrderKey, AccountOrigin, ProviderAccount};
use tokscope_core::Provider;

use super::action_button;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum AccountRemovalState {
    Ready,
    Confirming,
    Pending,
    Failed(SharedString),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct AccountRemovalView {
    pub(super) key: AccountOrderKey,
    pub(super) origin: AccountOrigin,
    pub(super) state: AccountRemovalState,
}

impl AccountRemovalView {
    pub(super) fn new(
        provider: Provider,
        account: &ProviderAccount,
        state: AccountRemovalState,
    ) -> Self {
        Self {
            key: AccountOrderKey::new(provider, account.id.as_str()),
            origin: account.origin(),
            state,
        }
    }
}

pub(super) type AccountRemovalHandler =
    Rc<dyn Fn(AccountOrderKey, &mut Window, &mut App) + 'static>;

pub(super) fn account_menu(
    view: AccountRemovalView,
    on_prompt: AccountRemovalHandler,
    on_remove: AccountRemovalHandler,
    on_cancel: AccountRemovalHandler,
    cx: &App,
) -> gpui::Div {
    let slug = view.key.provider.slug();
    let id = &view.key.account_id;
    let action_selector = format!("account-actions-{slug}-{id}");
    let state_selector = format!("account-removal-state-{slug}-{id}");
    let pending = matches!(&view.state, AccountRemovalState::Pending);
    let confirming = matches!(&view.state, AccountRemovalState::Confirming);
    let failure = match &view.state {
        AccountRemovalState::Failed(message) => Some(message.clone()),
        _ => None,
    };

    let menu_view = view.clone();
    let menu_handler = on_prompt.clone();
    let action = action_button(action_selector, cx)
        .compact()
        .child(div().text_sm().child("⋯"))
        .tooltip(if pending {
            "Removing account"
        } else {
            "Account actions"
        })
        .loading(pending)
        .disabled(pending)
        .dropdown_menu_with_anchor(Corner::TopRight, move |menu, _, _| {
            let item_view = menu_view.clone();
            let rendered_view = item_view.clone();
            let item_handler = menu_handler.clone();
            menu.min_w(px(220.)).item(
                PopupMenuItem::element(move |_, _| {
                    let selector = format!(
                        "remove-account-{}-{}",
                        rendered_view.key.provider.slug(),
                        rendered_view.key.account_id
                    );
                    div()
                        .debug_selector(move || selector.clone())
                        .size_full()
                        .flex()
                        .items_center()
                        .cursor_pointer()
                        .child("Remove account from Tokscope")
                })
                .on_click(move |_, window, cx| {
                    item_handler(item_view.key.clone(), window, cx);
                }),
            )
        });

    let confirmation = confirming.then(|| {
        let remove_key = view.key.clone();
        let remove_handler = on_remove.clone();
        let cancel_key = view.key.clone();
        let cancel_handler = on_cancel.clone();
        let body = confirmation_body(view.origin, view.key.provider.display_name());
        div()
            .debug_selector(|| "account-removal-confirmation".to_string())
            .flex()
            .items_center()
            .gap_1()
            .child(
                div()
                    .debug_selector(|| "account-removal-confirmation-copy".to_string())
                    .max_w(px(420.))
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(body),
            )
            .child(
                action_button(format!("cancel-remove-account-{slug}-{id}"), cx)
                    .compact()
                    .child("Cancel")
                    .on_click(move |_, window, cx| {
                        cancel_handler(cancel_key.clone(), window, cx);
                    }),
            )
            .child(
                action_button(format!("confirm-remove-account-{slug}-{id}"), cx)
                    .compact()
                    .child("Remove")
                    .on_click(move |_, window, cx| {
                        remove_handler(remove_key.clone(), window, cx);
                    }),
            )
    });

    div()
        .debug_selector(move || state_selector.clone())
        .flex()
        .items_center()
        .gap_2()
        .when_some(failure, |row, message| {
            row.child(
                div()
                    .max_w(px(240.))
                    .truncate()
                    .text_xs()
                    .text_color(cx.theme().danger)
                    .child(message),
            )
        })
        .when_some(confirmation, |row, confirmation| row.child(confirmation))
        .child(action)
}

fn confirmation_body(origin: AccountOrigin, provider: &str) -> String {
    let action = match origin {
        AccountOrigin::Managed => {
            "Its Tokscope-managed profile and saved credentials will be removed from this device."
        }
        AccountOrigin::Current => {
            "It will be hidden in Tokscope. Your current CLI credentials will not be changed."
        }
        AccountOrigin::Mixed => {
            "Tokscope-managed profiles will be removed and the current CLI account will be hidden. Your current CLI credentials will not be changed."
        }
        AccountOrigin::Unknown => {
            "It will be removed from Tokscope. Your provider credentials will not be changed."
        }
    };
    format!("{action} Your {provider} usage history will be kept.")
}

#[cfg(test)]
mod tests {
    use super::{confirmation_body, AccountOrigin};

    #[test]
    fn removal_copy_matches_capability() {
        assert!(confirmation_body(AccountOrigin::Managed, "Codex")
            .contains("saved credentials will be removed"));
        assert!(confirmation_body(AccountOrigin::Current, "Codex")
            .contains("credentials will not be changed"));
        assert!(confirmation_body(AccountOrigin::Mixed, "Claude Code")
            .contains("profiles will be removed"));
    }
}
