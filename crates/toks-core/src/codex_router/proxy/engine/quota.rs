use anyhow::Result;
use chrono::Utc;

use crate::accounts::AccountId;
use crate::rotation::{
    account_quota_drain, BlockWindow, FastLimitDisposition, FastLimitOutcome, ThreadId, UnixMillis,
    UsageLimitIncident,
};
use crate::storage::StoreUpdate;

use super::{now, Engine};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

mod snapshots;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AttemptedTier {
    ToksForcedFast,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResponseDelivery {
    NothingDelivered,
    Delivered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UsageLimitAction {
    RetrySameAccountAtStandardTier,
    TryAnotherAccount,
    ForwardFailure,
}

impl Engine {
    pub fn waiting(&self, thread: &ThreadId) -> Result<()> {
        if self.thread_sources.is_known_subagent(thread) {
            return Ok(());
        }
        self.mutate(|runtime| runtime.waiting(thread, now()))
    }

    pub fn release_reservation(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        self.mutate(|runtime| runtime.release_reservation(account, thread))
    }

    pub fn reserve_retry(&self, account: &AccountId, thread: &ThreadId) -> Result<()> {
        let at = now();
        self.runtime.update(
            |runtime| match runtime.reserve_thread(account, thread, at) {
                Ok(()) => StoreUpdate::Changed(Ok(())),
                Err(conflict) => StoreUpdate::Unchanged(Err(conflict)),
            },
        )??;
        Ok(())
    }

    /// The Fast tier accepted by `model`.
    pub fn fast_tier_for(&self, model: &str) -> Option<&'static str> {
        self.catalogue.fast_tier(model)
    }

    pub fn request_usage_limited(
        &self,
        account: &AccountId,
        thread: Option<&ThreadId>,
        tier: AttemptedTier,
        delivery: ResponseDelivery,
        reset: Option<UnixMillis>,
        incident: UsageLimitIncident,
    ) -> Result<UsageLimitAction> {
        debug_assert_eq!(incident.thread_id(), thread);
        if tier == AttemptedTier::ToksForcedFast {
            let Some(thread) = thread else {
                return Ok(UsageLimitAction::ForwardFailure);
            };
            let disposition = match delivery {
                ResponseDelivery::NothingDelivered => FastLimitDisposition::RetryingStandard,
                ResponseDelivery::Delivered => FastLimitDisposition::NextRequestUsesStandard,
            };
            let at = now();
            let window = block_window(account, reset);
            let outcome = self.runtime.update(|runtime| {
                let (outcome, _material_changed) =
                    runtime.fast_limit_reached(account, thread, window, disposition, at);
                runtime.usage_limited(account, incident, at);
                StoreUpdate::Changed(outcome)
            })?;
            return Ok(match (outcome, delivery) {
                (FastLimitOutcome::UseStandard, ResponseDelivery::NothingDelivered) => {
                    UsageLimitAction::RetrySameAccountAtStandardTier
                }
                (FastLimitOutcome::UseStandard, ResponseDelivery::Delivered) => {
                    UsageLimitAction::ForwardFailure
                }
                (FastLimitOutcome::AlreadyBlocked, ResponseDelivery::NothingDelivered) => {
                    UsageLimitAction::TryAnotherAccount
                }
                (FastLimitOutcome::AlreadyBlocked, ResponseDelivery::Delivered) => {
                    UsageLimitAction::ForwardFailure
                }
            });
        }

        let window = block_window(account, reset);
        let at = now();
        match thread {
            Some(thread) => self.runtime.update(|runtime| {
                runtime.thread_blocked(account, thread, window, at);
                runtime.usage_limited(account, incident, at);
                StoreUpdate::Changed(())
            })?,
            None => self.runtime.update(|runtime| {
                runtime.block_admission(account, window, at);
                runtime.usage_limited(account, incident, at);
                StoreUpdate::Changed(())
            })?,
        }
        Ok(match delivery {
            ResponseDelivery::NothingDelivered => UsageLimitAction::TryAnotherAccount,
            ResponseDelivery::Delivered => UsageLimitAction::ForwardFailure,
        })
    }

    #[cfg(test)]
    pub fn block_admission(&self, account: &AccountId, reset: Option<UnixMillis>) -> Result<()> {
        let at = now();
        let window = block_window(account, reset);
        self.runtime.update(|runtime| {
            runtime.block_admission(account, window, at);
            StoreUpdate::Changed(())
        })
    }

    pub fn upstream_admission_usage_limited(
        &self,
        account: &AccountId,
        reset: Option<UnixMillis>,
        incident: UsageLimitIncident,
    ) -> Result<()> {
        let at = now();
        let window = block_window(account, reset);
        self.runtime.update(|runtime| {
            runtime.block_admission(account, window, at);
            runtime.usage_limited(account, incident, at);
            StoreUpdate::Changed(())
        })
    }
}

fn block_window(account: &AccountId, reset: Option<UnixMillis>) -> BlockWindow {
    let at = now();
    reset
        .filter(|until| *until > at)
        .or_else(|| known_drain_reset(account).filter(|until| *until > at))
        .map_or_else(
            || BlockWindow::reprobe_at(UnixMillis::new(at.get() + REPROBE_AFTER_MILLIS)),
            BlockWindow::known,
        )
}

fn known_drain_reset(account: &AccountId) -> Option<UnixMillis> {
    let now = Utc::now();
    crate::limits::hydrate_all()
        .iter()
        .filter_map(|snapshot| account_quota_drain(snapshot, now))
        .find(|drain| &drain.account_id == account)
        .and_then(|drain| drain.reset_at)
}
