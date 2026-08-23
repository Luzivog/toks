use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};

use crate::accounts::AccountId;
use crate::limits::LimitSnapshot;
use crate::rotation::{
    account_quota_drain, BlockWindow, FastLimitDisposition, FastLimitOutcome, ThreadId, UnixMillis,
};

use super::{now, Engine};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

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
    pub fn apply_snapshots(
        &self,
        snapshots: &[LimitSnapshot],
        observed_at: DateTime<Utc>,
    ) -> Result<()> {
        let discovered = self.credentials.account_ids();
        let known: BTreeSet<_> = discovered.iter().cloned().collect();
        let draining = snapshots
            .iter()
            .filter_map(|snapshot| account_quota_drain(snapshot, observed_at))
            .filter(|drain| known.contains(&drain.account_id))
            .map(|drain| (drain.account_id, drain.reset_at))
            .collect::<BTreeMap<_, _>>();
        let at = now();
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        let before = runtime.clone();
        runtime.reconcile(&discovered, at);
        runtime.replace_quota_drain(&draining, at);
        runtime.heartbeat(at);
        if let Err(error) = self.runtime_store.save(&runtime) {
            *runtime = before;
            return Err(error);
        }
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
    ) -> Result<UsageLimitAction> {
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
            let mut runtime = self.runtime.lock().expect("router runtime poisoned");
            let before = runtime.clone();
            let (outcome, changed) =
                runtime.fast_limit_reached(account, thread, window, disposition, at);
            if changed {
                if let Err(error) = self
                    .runtime_store
                    .save(&runtime)
                    .context("saving router runtime")
                {
                    *runtime = before;
                    return Err(error);
                }
            }
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
            Some(thread) => {
                self.mutate(|runtime| runtime.thread_blocked(account, thread, window, at))?
            }
            None => self.mutate(|runtime| runtime.block_admission(account, window, at))?,
        }
        Ok(match delivery {
            ResponseDelivery::NothingDelivered => UsageLimitAction::TryAnotherAccount,
            ResponseDelivery::Delivered => UsageLimitAction::ForwardFailure,
        })
    }

    pub fn block_admission(&self, account: &AccountId, reset: Option<UnixMillis>) -> Result<()> {
        let at = now();
        let window = block_window(account, reset);
        self.mutate(|runtime| runtime.block_admission(account, window, at))
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
