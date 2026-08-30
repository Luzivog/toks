use crate::accounts::AccountId;
use crate::codex_router::proxy::lease::{StreamLease, ThreadAttachment};
use crate::codex_router::proxy::DIRECT_ROUTER_GENERATION;
use crate::rotation::{ThreadId, UnixMillis};
use crate::storage::StoreUpdate;

use super::Engines;

#[test]
fn tracked_leases_republish_exact_worker_presence() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("tracked-worker-presence");
    let worker = engines.worker(9, 909);

    let stream = StreamLease::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    let attachment = ThreadAttachment::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 1);

    worker.close(&account, &thread).unwrap();
    worker.detach(&account, &thread).unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 0);

    worker.reconcile_owned_connections().unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 1);

    drop(stream);
    drop(attachment);
    worker.reconcile_owned_connections().unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 0);
}

#[test]
fn failed_drop_publication_converges_from_the_lower_local_inventory() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("failed-drop-publication");
    let worker = engines.worker(10, 1_010);
    let stream = StreamLease::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    let attachment = ThreadAttachment::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();

    let runtime_directory = engines.store.path().parent().unwrap().to_owned();
    let preserved_directory = engines._directory.path().join("preserved-runtime");
    std::fs::rename(&runtime_directory, &preserved_directory).unwrap();
    std::fs::write(&runtime_directory, b"blocks runtime writes").unwrap();

    drop(stream);
    drop(attachment);

    std::fs::remove_file(&runtime_directory).unwrap();
    std::fs::rename(&preserved_directory, &runtime_directory).unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 1);

    worker.reconcile_owned_connections().unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 0);
}

#[test]
fn failed_continuation_publication_restores_attached_follow_up_presence() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("failed-continuation-publication");
    let worker = engines.worker(11, 1_111);
    let mut stream = StreamLease::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    let attachment = ThreadAttachment::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    stream.continue_after_response();

    let runtime_directory = engines.store.path().parent().unwrap().to_owned();
    let preserved_directory = engines._directory.path().join("preserved-runtime");
    std::fs::rename(&runtime_directory, &preserved_directory).unwrap();
    std::fs::write(&runtime_directory, b"blocks runtime writes").unwrap();
    drop(stream);
    std::fs::remove_file(&runtime_directory).unwrap();
    std::fs::rename(&preserved_directory, &runtime_directory).unwrap();

    worker.reconcile_owned_connections().unwrap();
    let runtime = engines.store.load().unwrap();
    assert_eq!(runtime.in_flight_count(&account), 0);
    let encoded = serde_json::to_value(&runtime).unwrap();
    assert!(encoded["attachedThreads"].get(thread.as_str()).is_some());

    drop(attachment);
    worker.reconcile_owned_connections().unwrap();
    let mut runtime = engines.store.load().unwrap();
    let encoded = serde_json::to_value(&runtime).unwrap();
    assert!(encoded["attachedThreads"].get(thread.as_str()).is_none());
    let challenger = AccountId::new("b");
    let conflict = runtime
        .connection_opened(&challenger, &thread, UnixMillis::new(50))
        .unwrap_err();
    assert_eq!(conflict.owned_by(), &account);
}

#[test]
fn successful_stream_open_supersedes_unpublished_local_follow_up_intent() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let thread = ThreadId::new("superseded-continuation-publication");
    let worker = engines.worker(12, 1_212);
    let mut first = StreamLease::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    first.continue_after_response();

    let runtime_directory = engines.store.path().parent().unwrap().to_owned();
    let preserved_directory = engines._directory.path().join("preserved-runtime");
    std::fs::rename(&runtime_directory, &preserved_directory).unwrap();
    std::fs::write(&runtime_directory, b"blocks runtime writes").unwrap();
    drop(first);
    std::fs::remove_file(&runtime_directory).unwrap();
    std::fs::rename(&preserved_directory, &runtime_directory).unwrap();

    let second = StreamLease::open(worker.clone(), &account, &thread, None)
        .unwrap()
        .unwrap();
    worker.reconcile_owned_connections().unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 1);

    drop(second);
    worker.reconcile_owned_connections().unwrap();
    assert!(!engines
        .store
        .load()
        .unwrap()
        .retained_thread_ids()
        .contains(&thread));
}

#[test]
fn direct_router_restart_adopts_reserved_ownership_and_clears_legacy_scalars() {
    let engines = Engines::new();
    let account = AccountId::new("a");
    let owned = ThreadId::new("dead-direct-owner");
    let legacy = ThreadId::new("legacy-direct-scalar");
    let predecessor = engines.worker(DIRECT_ROUTER_GENERATION, 100);
    let stranded = StreamLease::open(predecessor, &account, &owned, None)
        .unwrap()
        .unwrap();
    std::mem::forget(stranded);
    engines
        .first
        .runtime
        .update(|runtime| {
            runtime
                .connection_opened(&account, &legacy, UnixMillis::new(1))
                .unwrap();
            StoreUpdate::Changed(())
        })
        .unwrap();
    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 2);

    let _replacement = engines.worker(DIRECT_ROUTER_GENERATION, 101);

    assert_eq!(engines.store.load().unwrap().in_flight_count(&account), 0);
}
