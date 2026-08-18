use gpui::{div, prelude::*, px, Corner};
use gpui_component::{
    h_flex,
    menu::{DropdownMenu, PopupMenuItem},
    v_flex, ActiveTheme, StyledExt,
};
use tokscope_core::{LimitSnapshot, Provider, ProviderAccount};

use crate::window::{icon_element, TokscopeIcon};
use crate::TokscopeApp;

use super::{
    account_drop_target, account_limits_group, account_limits_loading_content, action_button,
    section_title,
};

pub(super) fn account_limits_section(
    app: &TokscopeApp,
    title: &'static str,
    cx: &mut gpui::Context<TokscopeApp>,
) -> gpui::Div {
    let app_handle = cx.entity().downgrade();
    let has_emails = app
        .limits
        .iter()
        .any(|snapshot| snapshot.account.email.is_some());
    let email_icon = if app.emails_hidden {
        TokscopeIcon::EyeOff
    } else {
        TokscopeIcon::Eye
    };
    let email_tooltip = if app.emails_hidden {
        "Show account emails"
    } else {
        "Hide account emails"
    };
    let toggle_emails = action_button("toggle-account-emails", cx)
        .compact()
        .child(icon_element(email_icon).size(px(14.)))
        .tooltip(email_tooltip)
        .on_click(cx.listener(|app, _, _, cx| {
            app.emails_hidden = !app.emails_hidden;
            cx.notify();
        }));
    let add_account = action_button("add-account", cx)
        .compact()
        .child(icon_element(TokscopeIcon::Plus).size(px(13.)))
        .child(div().text_sm().child("Add account"))
        .dropdown_menu_with_anchor(Corner::TopRight, move |mut menu, _, _| {
            menu = menu.label("Choose a provider").min_w(px(180.));
            for choice in Provider::ALL {
                let app_handle = app_handle.clone();
                menu = menu.item(
                    PopupMenuItem::element(move |_, _| {
                        div()
                            .debug_selector(move || {
                                format!("add-account-provider-{}", choice.slug())
                            })
                            .size_full()
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .child(choice.display_name())
                    })
                    .on_click(move |_, _, cx| {
                        let result = tokscope_core::accounts::begin_add_account(choice);
                        let _ = app_handle.update(cx, |app, cx| {
                            app.account_notice = Some(match result {
                                Ok(started) => {
                                    let account = ProviderAccount {
                                        id: started.account_id,
                                        email: None,
                                    };
                                    if !app.limits.iter().any(|snapshot| {
                                        snapshot.provider == started.provider
                                            && snapshot.account.id == account.id
                                    }) {
                                        app.limits.push(LimitSnapshot::loading_account(
                                            started.provider,
                                            account,
                                        ));
                                        tokscope_core::accounts::apply_saved_order(&mut app.limits);
                                    }
                                    format!(
                                        "Opened {} sign-in. The account email will appear automatically.",
                                        started.provider.display_name()
                                    )
                                }
                                Err(error) => {
                                    format!("Couldn't add {}: {error}", choice.display_name())
                                }
                            });
                            cx.notify();
                        });
                    }),
                );
            }
            menu
        });

    let mut panel = v_flex()
        .w_full()
        .overflow_hidden()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .px_3()
                .py_2()
                .justify_between()
                .items_center()
                .child(section_title(title))
                .child(
                    h_flex()
                        .gap_1()
                        .items_center()
                        .when(has_emails, |actions| actions.child(toggle_emails))
                        .child(add_account),
                ),
        );

    if let Some(notice) = &app.account_notice {
        panel = panel.child(
            div()
                .px_4()
                .py_2()
                .border_t_1()
                .border_color(cx.theme().border)
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(notice.clone()),
        );
    }

    let snapshots: Vec<&LimitSnapshot> = app.limits.iter().collect();
    if snapshots.is_empty() {
        if app.limits_loaded {
            return panel.child(
                v_flex()
                    .gap_1()
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .child(div().text_sm().font_semibold().child("No account data"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Add an account to see its plan limits here."),
                    ),
            );
        }

        return panel.child(account_limits_loading_content(cx));
    }

    let reorder_enabled = snapshots.len() > 1;
    for snapshot in snapshots {
        let group = account_limits_group(
            snapshot,
            app.now,
            true,
            app.emails_hidden,
            reorder_enabled,
            cx,
        );
        panel = if reorder_enabled {
            panel.child(account_drop_target(snapshot, group, cx))
        } else {
            panel.child(group)
        };
    }
    panel
}
