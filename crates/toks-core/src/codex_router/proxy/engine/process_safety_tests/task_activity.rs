use std::collections::BTreeMap;

use crate::codex_router::proxy::lease::{StreamLease, ThreadAttachment};
use crate::rotation::{TaskActivityStore, ThreadId};

use super::Engines;

#[test]
fn accepted_continue_and_detach_publish_the_exact_task_lifecycle() {
    let engines = Engines::new();
    let engine = engines.worker(6, 601);
    let account = &engines.accounts[0];
    let thread = ThreadId::new("tool-turn");
    let store = TaskActivityStore::for_data_dir(engines._directory.path());
    store
        .reconcile_expected_workers(&BTreeMap::from([(6, 601)]))
        .unwrap();
    let attachment = ThreadAttachment::open(engine.clone(), account, &thread, None)
        .unwrap()
        .unwrap();
    let mut stream = StreamLease::open(engine, account, &thread, None)
        .unwrap()
        .unwrap();

    assert_eq!(store.load().unwrap().active_task_rows().unwrap().len(), 1);
    stream.continue_after_response();
    drop(stream);
    assert_eq!(store.load().unwrap().active_task_rows().unwrap().len(), 1);

    drop(attachment);
    assert!(store.load().unwrap().active_task_rows().unwrap().is_empty());
}

#[test]
fn terminal_stream_drop_removes_the_task_without_detaching_the_socket() {
    let engines = Engines::new();
    let engine = engines.worker(8, 801);
    let account = &engines.accounts[0];
    let thread = ThreadId::new("final-turn");
    let store = TaskActivityStore::for_data_dir(engines._directory.path());
    store
        .reconcile_expected_workers(&BTreeMap::from([(8, 801)]))
        .unwrap();
    let _attachment = ThreadAttachment::open(engine.clone(), account, &thread, None)
        .unwrap()
        .unwrap();
    let stream = StreamLease::open(engine, account, &thread, None)
        .unwrap()
        .unwrap();
    assert_eq!(store.load().unwrap().active_task_rows().unwrap().len(), 1);

    drop(stream);

    assert!(store.load().unwrap().active_task_rows().unwrap().is_empty());
}
