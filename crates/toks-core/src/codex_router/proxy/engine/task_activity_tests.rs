use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::rotation::{
    TaskActivityStore, ThreadId, ThreadRequestSettings, UnixMillis, WorkerConnectionOwner,
};

use super::task_activity::TaskActivityPublisher;

struct Harness {
    _directory: tempfile::TempDir,
    store: TaskActivityStore,
    publisher: TaskActivityPublisher,
    account: AccountId,
    thread: ThreadId,
}

impl Harness {
    fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let store = TaskActivityStore::for_data_dir(directory.path());
        let owner = WorkerConnectionOwner::new(7, 70).unwrap();
        let publisher = TaskActivityPublisher::new(Some(owner), Some(store.clone()));
        store
            .reconcile_expected_workers(&BTreeMap::from([(7, 70)]))
            .unwrap();
        Self {
            _directory: directory,
            store,
            publisher,
            account: AccountId::new("account"),
            thread: ThreadId::new("thread"),
        }
    }

    fn start(&self) {
        self.publisher.started(
            &self.account,
            &self.thread,
            ThreadRequestSettings::default(),
            UnixMillis::now(),
        );
    }

    fn count(&self) -> usize {
        self.store.load().unwrap().active_task_rows().unwrap().len()
    }
}

#[test]
fn initial_snapshot_is_empty_and_accepted_start_is_active() {
    let test = Harness::new();
    assert_eq!(test.count(), 0);

    test.start();

    let rows = test.store.load().unwrap().active_task_rows().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].thread_id, test.thread);
    assert_eq!(rows[0].account_id, test.account);
}

#[test]
fn two_same_thread_leases_require_two_terminal_finishes() {
    let test = Harness::new();
    test.start();
    test.start();

    test.publisher.finished(&test.thread);
    assert_eq!(test.count(), 1);

    test.publisher.finished(&test.thread);
    assert_eq!(test.count(), 0);
}

#[test]
fn client_tool_continuation_stays_active_until_the_last_attachment_closes() {
    let test = Harness::new();
    test.publisher.attachment_opened(&test.thread);
    test.start();

    test.publisher.continues(&test.thread);
    assert_eq!(test.count(), 1);

    test.publisher.attachment_closed(&test.thread);
    assert_eq!(test.count(), 0);
}

#[test]
fn cancellation_clears_all_same_thread_leases() {
    let test = Harness::new();
    test.start();
    test.start();

    test.publisher.cancelled(&test.thread);

    assert_eq!(test.count(), 0);
}

#[test]
fn periodic_snapshot_preserves_the_exact_set() {
    let test = Harness::new();
    test.start();

    test.publisher.publish_current();

    assert_eq!(test.count(), 1);
}

#[test]
fn failed_topology_reconciliation_retries_without_interrupting_the_publisher() {
    let directory = tempfile::tempdir().unwrap();
    let blocker = directory.path().join("blocked");
    std::fs::write(&blocker, b"not a directory").unwrap();
    let store = TaskActivityStore::at(blocker.join("task-activity.json"));
    let owner = WorkerConnectionOwner::new(9, 90).unwrap();
    let publisher = TaskActivityPublisher::new(Some(owner), Some(store.clone()));
    let expected = BTreeMap::from([(9, 90)]);

    assert!(!publisher.reconcile_expected_workers(&expected));

    std::fs::remove_file(&blocker).unwrap();
    std::fs::create_dir(&blocker).unwrap();
    publisher.publish_current();
    assert!(publisher.reconcile_expected_workers(&expected));
    assert!(store.load().unwrap().active_task_rows().unwrap().is_empty());
}
