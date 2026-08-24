use std::collections::BTreeSet;
use std::fmt;

use crate::accounts::AccountId;

use crate::rotation::runtime::{RotationRuntime, ThreadId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThreadAccountConflict {
    requested: AccountId,
    owned_by: AccountId,
}

impl ThreadAccountConflict {
    pub fn requested(&self) -> &AccountId {
        &self.requested
    }

    pub fn owned_by(&self) -> &AccountId {
        &self.owned_by
    }
}

impl fmt::Display for ThreadAccountConflict {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "thread is owned by account {} and cannot run on account {}",
            self.owned_by, self.requested
        )
    }
}

impl std::error::Error for ThreadAccountConflict {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ThreadOwnership {
    Unowned,
    Owned(AccountId),
    Conflicting,
}

impl RotationRuntime {
    pub(in crate::rotation::runtime) fn claim_thread_account(
        &self,
        requested: &AccountId,
        thread: &ThreadId,
    ) -> Result<(), ThreadAccountConflict> {
        let conflicting = self
            .thread_accounts(thread)
            .into_iter()
            .find(|owner| owner != requested);
        match conflicting {
            Some(owned_by) => Err(ThreadAccountConflict {
                requested: requested.clone(),
                owned_by,
            }),
            None => Ok(()),
        }
    }

    pub(crate) fn thread_ownership(&self, thread: &ThreadId) -> ThreadOwnership {
        let mut accounts = self.thread_accounts(thread).into_iter();
        let Some(account) = accounts.next() else {
            return ThreadOwnership::Unowned;
        };
        if accounts.next().is_some() {
            ThreadOwnership::Conflicting
        } else {
            ThreadOwnership::Owned(account)
        }
    }

    fn thread_accounts(&self, thread: &ThreadId) -> BTreeSet<AccountId> {
        self.active_threads
            .get(thread)
            .map(|active| active.account_id.clone())
            .into_iter()
            .chain(
                self.attached_threads
                    .get(thread)
                    .filter(|attached| attached.connections() > 0)
                    .map(|attached| attached.account.clone()),
            )
            .collect()
    }
}
