use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::accounts::AccountId;
use crate::limits::LimitSnapshot;
use crate::rotation::{account_quota_exhaustion, ThreadId, UnixMillis};

use super::{now, Engine};

const REPROBE_AFTER_MILLIS: i64 = 60_000;

impl Engine {
    pub fn apply_snapshots(
        &self,
        snapshots: &[LimitSnapshot],
        observed_at: DateTime<Utc>,
    ) -> Result<()> {
        let discovered = self.credentials.account_ids();
        let known: BTreeSet<_> = discovered.iter().cloned().collect();
        let exhausted = snapshots
            .iter()
            .filter_map(|snapshot| account_quota_exhaustion(snapshot, observed_at))
            .filter(|exhaustion| known.contains(&exhaustion.account_id))
            .map(|exhaustion| (exhaustion.account_id, exhaustion.reset_at))
            .collect::<BTreeMap<_, _>>();
        let at = now();
        let mut runtime = self.runtime.lock().expect("router runtime poisoned");
        runtime.reconcile(&discovered, at);
        runtime.replace_quota_exhaustion(&exhausted, at);
        runtime.heartbeat(at);
        self.runtime_store.save(&runtime)
    }

    pub fn drains_in_place(&self, account: &AccountId, thread: &ThreadId) -> bool {
        let Ok(settings) = self.settings.load() else {
            return false;
        };
        settings.fast_when_draining()
            && !settings.excluded().contains(account)
            && self
                .runtime
                .lock()
                .expect("router runtime poisoned")
                .can_drain(account, thread, now())
    }

    /// The Fast tier `model` accepts, or `None` to keep the client's tier.
    pub fn fast_tier(&self, model: &str) -> Option<&'static str> {
        self.catalogue.fast_tier(model)
    }

    pub fn block(&self, account: &AccountId, reset: Option<UnixMillis>) -> Result<()> {
        let at = now();
        let known = reset.or_else(|| known_exhaustion_reset(account));
        let (until, reset_known) = known.map_or_else(
            || (UnixMillis::new(at.get() + REPROBE_AFTER_MILLIS), false),
            |until| (until, true),
        );
        self.mutate(|runtime| runtime.block(account, until, reset_known, at))
    }
}

fn known_exhaustion_reset(account: &AccountId) -> Option<UnixMillis> {
    let now = Utc::now();
    crate::limits::hydrate_all()
        .iter()
        .filter_map(|snapshot| account_quota_exhaustion(snapshot, now))
        .find(|exhaustion| &exhaustion.account_id == account)
        .and_then(|exhaustion| exhaustion.reset_at)
}
