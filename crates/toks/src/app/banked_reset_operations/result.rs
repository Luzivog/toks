use toks_core::{
    accounts::BankedResetResult,
    limits::{BankedResetAttempt, BankedResetOutcome},
    LimitSnapshot,
};

use super::{BankedResetOperations, State};

impl BankedResetOperations {
    pub(super) fn finish(
        &mut self,
        account: &toks_core::accounts::AccountId,
        attempt: &BankedResetAttempt,
        result: &anyhow::Result<BankedResetResult>,
    ) {
        let State::Pending {
            account: pending,
            attempt: pending_attempt,
            starting_count,
        } = &self.state
        else {
            return;
        };
        if pending != account || pending_attempt != attempt {
            return;
        }
        let starting_count = *starting_count;
        match result {
            Ok(result) => self.finish_result(account, *result),
            Err(error) => {
                self.state = State::Retry {
                    account: account.clone(),
                    attempt: attempt.clone(),
                    starting_count,
                    message: format!("Couldn't confirm the reset: {error}"),
                };
            }
        }
    }

    fn finish_result(
        &mut self,
        account: &toks_core::accounts::AccountId,
        result: BankedResetResult,
    ) {
        if result.outcome.used_credit() {
            if let Some(redeemed_at) = result.redeemed_at {
                self.redeemed.insert(account.clone(), redeemed_at);
            }
            self.error = (!result.routing_updated).then(routing_error);
        } else {
            self.error = Some(outcome_error(result.outcome).into());
        }
        self.state = State::Ready;
    }

    pub(crate) fn reconcile(&mut self, limits: &[LimitSnapshot]) {
        self.reconcile_with(
            limits,
            toks_core::accounts::acknowledge_observed_banked_reset,
        );
    }

    pub(super) fn reconcile_with(
        &mut self,
        limits: &[LimitSnapshot],
        acknowledge: impl FnOnce(
            &toks_core::accounts::AccountId,
        ) -> anyhow::Result<toks_core::rotation::UnixMillis>,
    ) {
        let State::Retry {
            account,
            starting_count,
            ..
        } = &self.state
        else {
            return;
        };
        let current = limits
            .iter()
            .find(|snapshot| &snapshot.account.id == account)
            .map(|snapshot| snapshot.banked_resets);
        if current.is_none_or(|count| count >= *starting_count) {
            return;
        }
        match acknowledge(account) {
            Ok(redeemed_at) => {
                self.redeemed.insert(account.clone(), redeemed_at);
                self.error = None;
                self.state = State::Ready;
            }
            Err(error) => {
                self.error = Some(format!(
                    "The reset was confirmed, but Codex routing could not be updated: {error}. Restart Codex routing."
                ));
            }
        }
    }
}

fn outcome_error(outcome: BankedResetOutcome) -> &'static str {
    match outcome {
        BankedResetOutcome::NothingToReset => "This account has no eligible limit to reset.",
        BankedResetOutcome::NoCredit => "No banked resets are available for this account.",
        BankedResetOutcome::Reset | BankedResetOutcome::AlreadyRedeemed => {
            unreachable!("used reset outcomes handled above")
        }
    }
}

fn routing_error() -> String {
    "The banked reset was used, but Toks couldn't update Codex routing. Restart Codex routing if the account stays blocked.".into()
}
