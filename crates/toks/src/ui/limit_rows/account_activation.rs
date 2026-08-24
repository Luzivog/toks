use std::rc::Rc;

use toks_core::{
    codex_router::account_activation::{self, ManualRequest},
    LimitSnapshot, Provider,
};

use crate::ToksApp;

use super::super::account_menu::{
    AccountActivationHandler, AccountActivationToggleHandler, AccountActivationView,
};

pub(super) fn view(snapshot: &LimitSnapshot) -> Option<AccountActivationView> {
    (snapshot.provider == Provider::Codex).then(|| AccountActivationView {
        account: snapshot.account.id.clone(),
        status: account_activation::status(&snapshot.account.id).ok(),
    })
}

pub(super) fn handlers(
    cx: &gpui::Context<ToksApp>,
) -> (AccountActivationHandler, AccountActivationToggleHandler) {
    let test_handle = cx.entity().downgrade();
    let test: AccountActivationHandler = Rc::new(move |account, _, cx| {
        let result = account_activation::request_test(&account);
        let _ = test_handle.update(cx, |app, cx| {
            match result {
                Ok(ManualRequest::Queued) => app.account_notice = None,
                Ok(ManualRequest::AlreadyRunning) => {
                    app.account_notice =
                        Some("A test is already in progress for this account.".into());
                }
                Err(error) => {
                    app.account_notice = Some(format!("Couldn't send test: {error}"));
                }
            }
            cx.notify();
        });
    });

    let toggle_handle = cx.entity().downgrade();
    let toggle: AccountActivationToggleHandler = Rc::new(move |account, enabled, _, cx| {
        let result = account_activation::set_automatic(&account, enabled);
        let _ = toggle_handle.update(cx, |app, cx| {
            match result {
                Ok(()) => app.account_notice = None,
                Err(error) => {
                    app.account_notice = Some(format!(
                        "Couldn't update automatic weekly reset tests: {error}"
                    ));
                }
            }
            cx.notify();
        });
    });
    (test, toggle)
}
