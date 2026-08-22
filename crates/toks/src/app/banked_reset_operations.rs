use toks_core::{accounts::BankedResetResult, limits::BankedResetAttempt, LimitSnapshot};

mod request;
pub(crate) use request::request_banked_reset;
use request::success_message;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BankedResetStatus {
    Ready,
    Busy,
    Confirming,
    Pending,
    Retry(String),
}

#[derive(Clone, Debug, Default)]
enum State {
    #[default]
    Ready,
    Confirming(toks_core::accounts::AccountId),
    Pending {
        account: toks_core::accounts::AccountId,
        attempt: BankedResetAttempt,
        starting_count: u64,
    },
    Retry {
        account: toks_core::accounts::AccountId,
        attempt: BankedResetAttempt,
        starting_count: u64,
        message: String,
    },
}

#[derive(Default)]
pub(crate) struct BankedResetOperations {
    state: State,
    notice: Option<String>,
}

impl BankedResetOperations {
    pub(crate) fn status(&self, account: &toks_core::accounts::AccountId) -> BankedResetStatus {
        match &self.state {
            State::Ready => BankedResetStatus::Ready,
            State::Confirming(candidate) if candidate == account => BankedResetStatus::Confirming,
            State::Pending {
                account: candidate, ..
            } if candidate == account => BankedResetStatus::Pending,
            State::Retry {
                account: candidate,
                message,
                ..
            } if candidate == account => BankedResetStatus::Retry(message.clone()),
            _ => BankedResetStatus::Busy,
        }
    }

    pub(crate) fn confirm(&mut self, account: toks_core::accounts::AccountId) {
        if matches!(self.state, State::Ready) {
            self.state = State::Confirming(account);
            self.notice = None;
        }
    }

    pub(crate) fn cancel(&mut self, account: &toks_core::accounts::AccountId) {
        if matches!(&self.state, State::Confirming(candidate) if candidate == account) {
            self.state = State::Ready;
        }
    }

    fn begin(
        &mut self,
        account: &toks_core::accounts::AccountId,
        starting_count: u64,
    ) -> Option<BankedResetAttempt> {
        let attempt = match &self.state {
            State::Confirming(candidate) if candidate == account => BankedResetAttempt::new(),
            State::Retry {
                account: candidate,
                attempt,
                ..
            } if candidate == account => attempt.clone(),
            _ => return None,
        };
        self.state = State::Pending {
            account: account.clone(),
            attempt: attempt.clone(),
            starting_count,
        };
        Some(attempt)
    }

    fn finish(
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
            Ok(result) => {
                self.notice = Some(success_message(*result));
                self.state = State::Ready;
            }
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

    pub(crate) fn reconcile(&mut self, limits: &[LimitSnapshot]) {
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
        if current.is_some_and(|count| count < *starting_count) {
            self.notice = Some("Banked reset confirmed by the refreshed account limits.".into());
            self.state = State::Ready;
        }
    }

    pub(crate) fn notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }

    pub(crate) fn dismiss_notice(&mut self) {
        self.notice = None;
    }
}

#[cfg(test)]
#[path = "banked_reset_operations/tests.rs"]
mod tests;
