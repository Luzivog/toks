use std::collections::BTreeMap;

use toks_core::{limits::BankedResetAttempt, rotation::UnixMillis};

mod request;
mod result;
pub(crate) use request::request_banked_reset;

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
    redeemed: BTreeMap<toks_core::accounts::AccountId, UnixMillis>,
    error: Option<String>,
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
            self.error = None;
        }
    }

    pub(crate) fn cancel(&mut self, account: &toks_core::accounts::AccountId) {
        if matches!(&self.state, State::Confirming(candidate) if candidate == account)
            || matches!(&self.state, State::Retry { account: candidate, .. } if candidate == account)
        {
            self.state = State::Ready;
            self.error = None;
        }
    }

    fn begin(
        &mut self,
        account: &toks_core::accounts::AccountId,
        starting_count: u64,
    ) -> Option<BankedResetAttempt> {
        let (attempt, starting_count) = match &self.state {
            State::Confirming(candidate) if candidate == account => {
                (BankedResetAttempt::new(), starting_count)
            }
            State::Retry {
                account: candidate,
                attempt,
                starting_count,
                ..
            } if candidate == account => (attempt.clone(), *starting_count),
            _ => return None,
        };
        self.state = State::Pending {
            account: account.clone(),
            attempt: attempt.clone(),
            starting_count,
        };
        Some(attempt)
    }

    pub(crate) fn redeemed_at(
        &self,
        account: &toks_core::accounts::AccountId,
    ) -> Option<UnixMillis> {
        self.redeemed.get(account).copied()
    }

    pub(crate) fn error(&self) -> Option<&str> {
        self.error.as_deref().or(match &self.state {
            State::Retry { message, .. } => Some(message.as_str()),
            _ => None,
        })
    }
}

#[cfg(test)]
#[path = "banked_reset_operations/tests.rs"]
mod tests;
