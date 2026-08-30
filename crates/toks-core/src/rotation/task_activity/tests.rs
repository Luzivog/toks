use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::rotation::{ThreadId, ThreadRequestSettings, UnixMillis, WorkerConnectionOwner};

use super::{
    ActiveTask, TaskActivity, TaskActivityConflict, TaskActivityStore, TaskActivityUnavailable,
    TASK_ACTIVITY_FRESHNESS_MILLIS,
};

fn owner(generation: u64, instance: u64) -> WorkerConnectionOwner {
    WorkerConnectionOwner::new(generation, instance).unwrap()
}

fn task(account: &str, started_at: i64) -> ActiveTask {
    ActiveTask {
        account_id: AccountId::new(account),
        request_settings: ThreadRequestSettings::default(),
        started_at: UnixMillis::new(started_at),
    }
}

fn tasks(entries: &[(&str, &str, i64)]) -> BTreeMap<ThreadId, ActiveTask> {
    entries
        .iter()
        .map(|(thread, account, started_at)| (ThreadId::new(*thread), task(account, *started_at)))
        .collect()
}

#[test]
fn unknown_incomplete_and_stale_coverage_are_unavailable() {
    let mut activity = TaskActivity::default();
    assert_eq!(
        activity.active_task_rows_at(UnixMillis::new(0)),
        Err(TaskActivityUnavailable::TopologyUnknown)
    );

    activity
        .reconcile_expected_workers(&BTreeMap::from([(1, 10)]))
        .unwrap();
    assert_eq!(
        activity.active_task_rows_at(UnixMillis::new(0)),
        Err(TaskActivityUnavailable::MissingWorker { generation: 1 })
    );

    activity
        .replace_worker_at(
            owner(1, 10),
            0,
            tasks(&[("a", "account", 0)]),
            UnixMillis::new(0),
        )
        .unwrap();
    assert!(activity
        .active_task_rows_at(UnixMillis::new(TASK_ACTIVITY_FRESHNESS_MILLIS))
        .is_ok());
    assert_eq!(
        activity.active_task_rows_at(UnixMillis::new(TASK_ACTIVITY_FRESHNESS_MILLIS + 1)),
        Err(TaskActivityUnavailable::StaleWorker { generation: 1 })
    );
}

#[test]
fn same_revision_heartbeat_refreshes_but_cannot_change_the_set() {
    let worker = owner(1, 10);
    let exact = tasks(&[("a", "account", 1)]);
    let mut activity = TaskActivity::default();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(1, 10)]))
        .unwrap();
    assert!(activity
        .replace_worker_at(worker, 0, exact.clone(), UnixMillis::new(1))
        .unwrap());
    assert!(activity
        .replace_worker_at(worker, 0, exact.clone(), UnixMillis::new(10))
        .unwrap());
    assert!(activity
        .active_task_rows_at(UnixMillis::new(10 + TASK_ACTIVITY_FRESHNESS_MILLIS))
        .is_ok());
    assert!(!activity
        .replace_worker_at(worker, 0, exact, UnixMillis::new(10))
        .unwrap());
    assert_eq!(
        activity
            .replace_worker_at(
                worker,
                0,
                tasks(&[("different", "account", 1)]),
                UnixMillis::new(11),
            )
            .unwrap_err(),
        TaskActivityConflict::RevisionReused {
            generation: 1,
            revision: 0,
        }
    );
}

#[test]
fn exact_revisions_replace_the_set_and_stale_revisions_cannot_restore_it() {
    let worker = owner(1, 10);
    let mut activity = TaskActivity::default();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(1, 10)]))
        .unwrap();
    activity
        .replace_worker_at(
            worker,
            1,
            tasks(&[("parent", "account", 1), ("child", "account", 2)]),
            UnixMillis::new(10),
        )
        .unwrap();
    activity
        .replace_worker_at(
            worker,
            2,
            tasks(&[("parent", "account", 1)]),
            UnixMillis::new(11),
        )
        .unwrap();
    assert!(!activity
        .replace_worker_at(
            worker,
            1,
            tasks(&[("parent", "account", 1), ("child", "account", 2)]),
            UnixMillis::new(12),
        )
        .unwrap());
    let rows = activity.active_task_rows_at(UnixMillis::new(12)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].thread_id.as_str(), "parent");
}

#[test]
fn expected_instance_prevents_same_generation_snapshot_flapping() {
    let old = owner(1, 10);
    let replacement = owner(1, 11);
    let mut activity = TaskActivity::default();
    activity
        .replace_worker_at(old, 1, tasks(&[("old", "account", 1)]), UnixMillis::new(10))
        .unwrap();
    activity
        .replace_worker_at(
            replacement,
            0,
            tasks(&[("new", "account", 2)]),
            UnixMillis::new(10),
        )
        .unwrap();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(1, 11)]))
        .unwrap();
    activity
        .replace_worker_at(old, 1, tasks(&[("old", "account", 1)]), UnixMillis::new(11))
        .unwrap();

    let rows = activity.active_task_rows_at(UnixMillis::new(11)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].thread_id.as_str(), "new");
}

#[test]
fn cross_worker_disagreement_fails_closed_and_generation_projection_is_exact() {
    let mut activity = TaskActivity::default();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(1, 10), (2, 20)]))
        .unwrap();
    activity
        .replace_worker_at(
            owner(1, 10),
            0,
            tasks(&[("same", "a", 4)]),
            UnixMillis::new(10),
        )
        .unwrap();
    activity
        .replace_worker_at(
            owner(2, 20),
            0,
            tasks(&[("same", "b", 5)]),
            UnixMillis::new(10),
        )
        .unwrap();
    assert_eq!(
        activity.active_task_rows_at(UnixMillis::new(10)),
        Err(TaskActivityUnavailable::ConflictingTask {
            thread_id: ThreadId::new("same"),
        })
    );

    activity
        .replace_worker_at(
            owner(2, 20),
            1,
            tasks(&[("other", "b", 5)]),
            UnixMillis::new(11),
        )
        .unwrap();
    let generations = activity
        .generation_activity_at(UnixMillis::new(11))
        .unwrap();
    assert_eq!(generations[&1].task_count, 1);
    assert_eq!(generations[&1].oldest_task_at, Some(UnixMillis::new(4)));
    assert_eq!(generations[&2].task_count, 1);
}

#[test]
fn separate_store_round_trips_atomic_worker_updates_with_private_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let directory = tempfile::tempdir().unwrap();
    let store = TaskActivityStore::for_data_dir(directory.path());
    store
        .replace_worker_at(
            owner(1, 10),
            0,
            tasks(&[("one", "a", 1)]),
            UnixMillis::new(10),
        )
        .unwrap();
    store
        .replace_worker_at(
            owner(2, 20),
            0,
            tasks(&[("two", "b", 2)]),
            UnixMillis::new(10),
        )
        .unwrap();
    store
        .reconcile_expected_workers(&BTreeMap::from([(1, 10), (2, 20)]))
        .unwrap();

    let restored = store.load().unwrap();
    assert_eq!(
        restored
            .active_task_rows_at(UnixMillis::new(10))
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        std::fs::metadata(store.path())
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert!(!directory.path().join("rotation/runtime.json").exists());
}

#[test]
fn concurrent_store_replacements_do_not_lose_a_worker_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let store = TaskActivityStore::for_data_dir(directory.path());
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    std::thread::scope(|scope| {
        for (generation, thread) in [(1, "one"), (2, "two")] {
            let store = store.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                store
                    .replace_worker_at(
                        owner(generation, generation * 10),
                        0,
                        tasks(&[(thread, "account", generation as i64)]),
                        UnixMillis::new(10),
                    )
                    .unwrap();
            });
        }
    });
    store
        .reconcile_expected_workers(&BTreeMap::from([(1, 10), (2, 20)]))
        .unwrap();

    assert_eq!(
        store
            .load()
            .unwrap()
            .active_task_rows_at(UnixMillis::new(10))
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn unsupported_store_version_is_rejected() {
    let directory = tempfile::tempdir().unwrap();
    let store = TaskActivityStore::for_data_dir(directory.path());
    std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    std::fs::write(store.path(), br#"{"version":2,"workers":{}}"#).unwrap();

    assert!(store
        .load()
        .unwrap_err()
        .to_string()
        .contains("unsupported task activity version 2"));
}

#[test]
fn unknown_persisted_fields_are_rejected_instead_of_erased() {
    let directory = tempfile::tempdir().unwrap();
    let store = TaskActivityStore::for_data_dir(directory.path());
    std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    std::fs::write(
        store.path(),
        br#"{"version":1,"workers":{},"futureField":true}"#,
    )
    .unwrap();

    let error = store.load().unwrap_err();
    assert!(format!("{error:#}").contains("unknown field"));
}

#[test]
fn unknown_nested_request_settings_fields_are_rejected_instead_of_erased() {
    let directory = tempfile::tempdir().unwrap();
    let store = TaskActivityStore::for_data_dir(directory.path());
    let worker = owner(1, 10);
    let mut activity = TaskActivity::default();
    activity
        .replace_worker_at(
            worker,
            0,
            tasks(&[("thread", "account", 0)]),
            UnixMillis::new(0),
        )
        .unwrap();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(1, 10)]))
        .unwrap();
    let mut persisted = serde_json::to_value(activity).unwrap();
    persisted["workers"]["1"]["10"]["tasks"]["thread"]["requestSettings"]["futureField"] =
        serde_json::json!(true);
    std::fs::create_dir_all(store.path().parent().unwrap()).unwrap();
    std::fs::write(store.path(), serde_json::to_vec(&persisted).unwrap()).unwrap();

    let error = store.load().unwrap_err();
    assert!(format!("{error:#}").contains("unknown field"));
}
