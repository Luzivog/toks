use std::rc::Rc;

use gpui::{div, prelude::*, px, App, Corner, SharedString, Window};
use gpui_component::{
    menu::{DropdownMenu, PopupMenuItem},
    ActiveTheme, Disableable,
};
use toks_core::accounts::{AccountOrderKey, AccountOrigin, ProviderAccount};
use toks_core::Provider;

use super::action_button;

mod activation;
#[cfg(test)]
mod activation_tests;
pub(super) use activation::{
    AccountActivationHandler, AccountActivationToggleHandler, AccountActivationView,
};
mod removal;
#[cfg(test)]
mod removal_tests;
use removal::confirmation_body;

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

pub(super) struct AccountMenuHandlers {
    pub(super) test: AccountActivationHandler,
    pub(super) toggle_automatic: AccountActivationToggleHandler,
    pub(super) prompt_removal: AccountRemovalHandler,
    pub(super) remove: AccountRemovalHandler,
    pub(super) cancel_removal: AccountRemovalHandler,
}

pub(super) fn account_menu(
    view: AccountRemovalView,
    activation: Option<AccountActivationView>,
    handlers: AccountMenuHandlers,
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
    let menu_handler = handlers.prompt_removal.clone();
    let activation_view = activation.clone();
    let test_handler = handlers.test.clone();
    let toggle_handler = handlers.toggle_automatic.clone();
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
            let mut menu = menu.min_w(px(220.));
            if let Some(view) = activation_view.clone() {
                for item in activation::items(view, test_handler.clone(), toggle_handler.clone()) {
                    menu = menu.item(item);
                }
                menu = menu.separator();
            }
            let item_view = menu_view.clone();
            let rendered_view = item_view.clone();
            let item_handler = menu_handler.clone();
            menu.item(
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
                        .child("Remove account from Toks")
                })
                .on_click(move |_, window, cx| {
                    item_handler(item_view.key.clone(), window, cx);
                }),
            )
        });

    let confirmation = confirming.then(|| {
        let remove_key = view.key.clone();
        let remove_handler = handlers.remove.clone();
        let cancel_key = view.key.clone();
        let cancel_handler = handlers.cancel_removal.clone();
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
