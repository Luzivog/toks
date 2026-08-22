use gpui::{AppContext, Context};
use toks_core::{
    accounts::{BankedResetResult, ProviderAccount},
    limits::BankedResetOutcome,
};

use crate::ToksApp;

pub(crate) fn request_banked_reset(
    app: &mut ToksApp,
    account: ProviderAccount,
    starting_count: u64,
    cx: &mut Context<ToksApp>,
) {
    let Some(attempt) = app.banked_resets.begin(&account.id, starting_count) else {
        return;
    };
    let account_id = account.id.clone();
    cx.notify();
    cx.spawn(async move |this, cx| {
        let request_attempt = attempt.clone();
        let result = cx
            .background_spawn(async move {
                toks_core::accounts::redeem_banked_reset(&account, &request_attempt)
            })
            .await;
        let _ = this.update(cx, |app, cx| {
            let used_credit = result
                .as_ref()
                .is_ok_and(|result| result.outcome.used_credit());
            app.banked_resets.finish(&account_id, &attempt, &result);
            if used_credit {
                if let Some(snapshot) = app
                    .limits
                    .iter_mut()
                    .find(|snapshot| snapshot.account.id == account_id)
                {
                    snapshot.banked_resets = snapshot.banked_resets.saturating_sub(1);
                    snapshot.banked_reset_credits = None;
                }
            }
            cx.notify();
        });
    })
    .detach();
}

pub(super) fn success_message(result: BankedResetResult) -> String {
    let message = match result.outcome {
        BankedResetOutcome::Reset => "Banked reset used. Codex limits will refresh shortly.",
        BankedResetOutcome::AlreadyRedeemed => {
            "This reset request already completed. Codex limits will refresh shortly."
        }
        BankedResetOutcome::NothingToReset => "This account has no eligible limit to reset.",
        BankedResetOutcome::NoCredit => "No banked resets are available for this account.",
    };
    if result.outcome.used_credit() && !result.routing_updated {
        format!("{message} Restart Codex routing if the account stays blocked.")
    } else {
        message.into()
    }
}
