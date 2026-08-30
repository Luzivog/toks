use crate::accounts::AccountId;

use super::{
    RotationRuntime, ThreadId, ThreadRequestSettings, UnixMillis, WorkerConnectionInventory,
    WorkerConnectionOwner,
};

fn owner(generation: u64, instance: u64) -> WorkerConnectionOwner {
    WorkerConnectionOwner::new(generation, instance).unwrap()
}

#[test]
fn worker_inventory_replaces_only_that_workers_connection_counts() {
    let account = AccountId::new("a");
    let thread = ThreadId::new("shared-thread");
    let worker = owner(1, 101);
    let sibling = owner(2, 202);
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));

    for _ in 0..2 {
        runtime
            .connection_opened_by(worker, &account, &thread, UnixMillis::new(1))
            .unwrap();
        runtime
            .thread_attached_by(worker, &account, &thread)
            .unwrap();
    }
    runtime
        .connection_opened_by(sibling, &account, &thread, UnixMillis::new(2))
        .unwrap();
    runtime
        .thread_attached_by(sibling, &account, &thread)
        .unwrap();

    let mut inventory = WorkerConnectionInventory::default();
    inventory.stream_opened(&account, &thread);

    assert!(runtime
        .reconcile_worker_connection_inventory(worker, &inventory, UnixMillis::new(3))
        .unwrap());
    assert!(!runtime
        .reconcile_worker_connection_inventory(worker, &inventory, UnixMillis::new(4))
        .unwrap());

    assert!(runtime.connection_closed_by(worker, &account, &thread, UnixMillis::new(5)));
    assert!(!runtime.connection_closed_by(worker, &account, &thread, UnixMillis::new(6)));
    assert!(!runtime.thread_detached_by(worker, &account, &thread));

    assert!(runtime.connection_closed_by(sibling, &account, &thread, UnixMillis::new(7)));
    assert!(runtime.thread_detached_by(sibling, &account, &thread));
    assert_eq!(runtime.in_flight_count(&account), 0);
}

#[test]
fn worker_inventory_restores_missing_presence_without_erasing_affinity_metadata() {
    let account = AccountId::new("a");
    let other = AccountId::new("b");
    let affinity = ThreadId::new("retained-affinity");
    let restored = ThreadId::new("restored-presence");
    let worker = owner(7, 707);
    let challenger = owner(8, 808);
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[account.clone(), other.clone()], UnixMillis::new(0));
    runtime
        .connection_opened_by_observed(
            worker,
            &account,
            &affinity,
            UnixMillis::new(1),
            ThreadRequestSettings {
                model: Some("gpt-test".into()),
                ..ThreadRequestSettings::default()
            },
        )
        .unwrap();
    assert!(runtime.connection_continues_by(worker, &account, &affinity, UnixMillis::new(2)));

    let mut inventory = WorkerConnectionInventory::default();
    inventory.stream_opened(&account, &restored);
    inventory.attachment_opened(&account, &restored);

    assert!(runtime
        .reconcile_worker_connection_inventory(worker, &inventory, UnixMillis::new(3))
        .unwrap());
    assert_eq!(runtime.in_flight_count(&account), 1);
    assert_eq!(runtime.live_thread_rows()[0].thread_id, restored);
    assert_eq!(
        runtime.thread_request_settings(&affinity).unwrap().model,
        Some("gpt-test".into())
    );

    let conflict = runtime
        .connection_opened_by(challenger, &other, &affinity, UnixMillis::new(4))
        .unwrap_err();
    assert_eq!(conflict.owned_by(), &account);
}
