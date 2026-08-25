use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use chrono::{TimeZone, Utc};

use crate::accounts::ProviderLimitCollection;
use crate::codex_router::proxy::engine::SnapshotApplication;
use crate::codex_router::proxy::heartbeat::refresh_quota;
use crate::limits::live::{memoized_or_fetch, RefreshOutcome};
use crate::rotation::{AccountAvailability, ThreadId, UnixMillis};
use crate::storage::StoreUpdate;

use super::quota_snapshot_tests::QuotaFixture;

#[tokio::test]
async fn one_heartbeat_evicts_an_older_memo_and_refetches_before_applying_quota() {
    let fixture = QuotaFixture::new();
    let engine = fixture.engine(true);
    let fetched_at = Utc.timestamp_millis_opt(1_000).single().unwrap();
    let acknowledged_at = UnixMillis::new(2_000);
    let thread = ThreadId::new("existing");
    let mut stale = fixture.snapshot(99.0);
    stale.fetched_at = Some(fetched_at);
    let stale_outcome = RefreshOutcome {
        snapshot: Some(stale),
        issue: None,
        codex_auth: Some(fixture.proof()),
    };
    let fetches = Arc::new(AtomicUsize::new(0));
    memoized_or_fetch(&fixture.profile, || {
        fetches.fetch_add(1, Ordering::SeqCst);
        stale_outcome.clone()
    });
    engine
        .runtime
        .update(|runtime| {
            runtime.thread_attached(&fixture.account, &thread).unwrap();
            runtime.banked_reset_consumed(&fixture.account, acknowledged_at);
            StoreUpdate::Changed(())
        })
        .unwrap();

    let profile = fixture.profile.clone();
    let fresh = fresh_outcome(&fixture, acknowledged_at, 50.0);
    let collector_fetches = Arc::clone(&fetches);
    let applied = refresh_quota(&engine, move || {
        let outcome = memoized_or_fetch(&profile, || {
            collector_fetches.fetch_add(1, Ordering::SeqCst);
            fresh.clone()
        });
        collection(outcome)
    })
    .await;

    assert!(applied.is_some());
    assert_eq!(
        fetches.load(Ordering::SeqCst),
        2,
        "the heartbeat refetched after evicting the stale memo"
    );
    assert_eq!(
        fixture.store.load().unwrap().accounts()[&fixture.account].availability(acknowledged_at),
        AccountAvailability::Available
    );
    assert!(!fixture
        .store
        .load()
        .unwrap()
        .can_drain(&fixture.account, &thread, acknowledged_at));
}

#[test]
fn a_strictly_post_reset_snapshot_can_start_a_real_drain() {
    let fixture = QuotaFixture::new();
    let engine = fixture.engine(true);
    let acknowledged_at = UnixMillis::new(2_000);
    let thread = ThreadId::new("existing");
    engine
        .runtime
        .update(|runtime| {
            runtime.thread_attached(&fixture.account, &thread).unwrap();
            runtime.banked_reset_consumed(&fixture.account, acknowledged_at);
            StoreUpdate::Changed(())
        })
        .unwrap();
    let epoch = engine.begin_snapshot_refresh().unwrap();
    let fresh = fresh_outcome(&fixture, acknowledged_at, 99.0);

    assert_eq!(
        engine
            .apply_snapshots(&collection(fresh), &epoch, Utc::now())
            .unwrap(),
        SnapshotApplication::Applied
    );
    assert!(matches!(
        fixture.store.load().unwrap().accounts()[&fixture.account].availability(acknowledged_at),
        AccountAvailability::Draining { .. }
    ));
    assert!(fixture
        .store
        .load()
        .unwrap()
        .can_drain(&fixture.account, &thread, acknowledged_at));
}

fn fresh_outcome(
    fixture: &QuotaFixture,
    acknowledged_at: UnixMillis,
    percent_used: f64,
) -> RefreshOutcome {
    let mut snapshot = fixture.snapshot(percent_used);
    snapshot.fetched_at = Utc.timestamp_millis_opt(acknowledged_at.get() + 1).single();
    RefreshOutcome {
        snapshot: Some(snapshot),
        issue: None,
        codex_auth: Some(fixture.proof()),
    }
}

fn collection(outcome: RefreshOutcome) -> ProviderLimitCollection {
    ProviderLimitCollection {
        snapshots: outcome.snapshot.into_iter().collect(),
        codex_auth: outcome.codex_auth.into_iter().collect(),
    }
}
