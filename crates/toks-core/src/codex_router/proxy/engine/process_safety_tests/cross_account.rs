use std::sync::Arc;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, UnixMillis, WorkerConnectionOwner};
use crate::storage::StoreUpdate;

use super::Engines;

#[tokio::test]
async fn live_ownership_blocks_cross_account_execution_until_complete_release() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let a = AccountId::new("a");
    let b = AccountId::new("b");
    let thread = ThreadId::new("one-thread-two-generations");
    let first = engines.worker(1, 101);
    let second = engines.worker(2, 201);
    assert!(first.route(&a, &thread).unwrap().is_some());
    assert!(first.attach(&a, &thread).unwrap());
    engines.prioritize(&b);
    let owned_by_a = engines.store.load().unwrap();

    let reservation = second
        .runtime
        .update(|runtime| {
            let outcome = runtime.reserve_thread(&b, &thread, UnixMillis::new(2));
            let changed = outcome.is_ok();
            StoreUpdate::from_changed(outcome, changed)
        })
        .unwrap()
        .unwrap_err();
    assert_eq!(reservation.owned_by(), &a);
    assert_eq!(engines.store.load().unwrap(), owned_by_a);

    let stream = second
        .runtime
        .update(|runtime| {
            let outcome = runtime.connection_opened_by(
                WorkerConnectionOwner::new(2, 201).unwrap(),
                &b,
                &thread,
                UnixMillis::new(3),
            );
            let changed = outcome.is_ok();
            StoreUpdate::from_changed(outcome, changed)
        })
        .unwrap()
        .unwrap_err();
    assert_eq!(stream.owned_by(), &a);
    assert_eq!(engines.store.load().unwrap(), owned_by_a);

    let attachment = second
        .runtime
        .update(|runtime| {
            let outcome = runtime.thread_attached_by(
                WorkerConnectionOwner::new(2, 201).unwrap(),
                &b,
                &thread,
            );
            let changed = outcome.as_ref().is_ok_and(|changed| *changed);
            StoreUpdate::from_changed(outcome, changed)
        })
        .unwrap()
        .unwrap_err();
    assert_eq!(attachment.owned_by(), &a);
    assert_eq!(engines.store.load().unwrap(), owned_by_a);

    assert!(second.route(&b, &thread).unwrap().is_none());
    assert!(!second.attach(&b, &thread).unwrap());
    assert_eq!(engines.store.load().unwrap(), owned_by_a);
    let skipped = std::collections::BTreeSet::from([a.clone()]);
    assert!(second
        .select_for_thread(Some(&thread), &skipped)
        .await
        .unwrap()
        .is_none());
    assert_eq!(engines.store.load().unwrap(), owned_by_a);

    first.close(&a, &thread).unwrap();
    assert!(second.route(&b, &thread).unwrap().is_none());
    assert!(!second.attach(&b, &thread).unwrap());
    first.detach(&a, &thread).unwrap();

    let selected = second
        .select_for_thread(Some(&thread), &Default::default())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.account_id, b);
    assert!(second.route(&b, &thread).unwrap().is_some());
    assert!(second.attach(&b, &thread).unwrap());
    let runtime = engines.store.load().unwrap();
    assert_eq!(runtime.in_flight_count(&a), 0);
    assert_eq!(runtime.in_flight_count(&b), 1);
}

#[test]
fn concurrent_cross_account_reservations_have_exactly_one_winner() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let first = engines.worker(1, 101);
    let second = engines.worker(2, 201);
    let thread = ThreadId::new("concurrent-cross-account-reservation");
    let start = Arc::new(std::sync::Barrier::new(3));
    let workers =
        [(first, AccountId::new("a")), (second, AccountId::new("b"))].map(|(engine, account)| {
            let thread = thread.clone();
            let start = start.clone();
            std::thread::spawn(move || {
                start.wait();
                engine
                    .runtime
                    .update(|runtime| {
                        let outcome = runtime.reserve_thread(&account, &thread, UnixMillis::new(1));
                        let changed = outcome.is_ok();
                        StoreUpdate::from_changed(outcome, changed)
                    })
                    .unwrap()
            })
        });
    start.wait();
    let outcomes = workers.map(|worker| worker.join().unwrap());

    assert_eq!(outcomes.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(outcomes.iter().filter(|result| result.is_err()).count(), 1);
    let runtime = engines.store.load().unwrap();
    assert_eq!(
        runtime.in_flight_count(&AccountId::new("a"))
            + runtime.in_flight_count(&AccountId::new("b")),
        1
    );
}
