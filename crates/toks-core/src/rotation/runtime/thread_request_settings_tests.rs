use std::collections::BTreeMap;

use crate::accounts::AccountId;
use crate::rotation::{ActiveTask, TaskActivity};

use super::{RotationRuntime, ThreadId, ThreadRequestSettings, UnixMillis, WorkerConnectionOwner};

fn account(id: &str) -> AccountId {
    AccountId::new(id)
}

fn owner(generation: u64, instance: u64) -> WorkerConnectionOwner {
    WorkerConnectionOwner::new(generation, instance).unwrap()
}

fn active_task(account_id: &AccountId, started_at: i64) -> ActiveTask {
    ActiveTask {
        account_id: account_id.clone(),
        request_settings: ThreadRequestSettings::default(),
        started_at: UnixMillis::new(started_at),
    }
}

#[test]
fn exact_activity_ignores_twenty_five_stale_transport_claims() {
    let account = account("account");
    let running = ThreadId::new("running");
    let mut runtime = RotationRuntime::default();
    for index in 0..25 {
        let thread = if index == 0 {
            running.clone()
        } else {
            ThreadId::new(format!("stale-{index}"))
        };
        runtime
            .connection_opened(&account, &thread, UnixMillis::new(index))
            .unwrap();
    }
    assert_eq!(runtime.in_flight_count(&account), 25);

    let mut activity = TaskActivity::default();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(23, 2_301)]))
        .unwrap();
    activity
        .replace_worker_at(
            owner(23, 2_301),
            1,
            BTreeMap::from([(running.clone(), active_task(&account, 1))]),
            UnixMillis::new(10),
        )
        .unwrap();

    let rows = activity.active_task_rows_at(UnixMillis::new(10)).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].thread_id, running);
    assert_eq!(rows[0].account_id, account);
}

#[test]
fn exact_replacement_removes_terminal_children_and_parents() {
    let account = account("account");
    let parent = ThreadId::new("parent");
    let child = ThreadId::new("child");
    let worker = owner(24, 2_401);
    let mut activity = TaskActivity::default();
    activity
        .reconcile_expected_workers(&BTreeMap::from([(24, 2_401)]))
        .unwrap();
    activity
        .replace_worker_at(
            worker,
            1,
            BTreeMap::from([
                (parent.clone(), active_task(&account, 1)),
                (child, active_task(&account, 2)),
            ]),
            UnixMillis::new(10),
        )
        .unwrap();
    assert_eq!(
        activity
            .active_task_rows_at(UnixMillis::new(10))
            .unwrap()
            .len(),
        2
    );

    activity
        .replace_worker_at(
            worker,
            2,
            BTreeMap::from([(parent.clone(), active_task(&account, 1))]),
            UnixMillis::new(11),
        )
        .unwrap();
    assert_eq!(
        activity.active_task_rows_at(UnixMillis::new(11)).unwrap()[0].thread_id,
        parent
    );

    activity
        .replace_worker_at(worker, 3, BTreeMap::new(), UnixMillis::new(12))
        .unwrap();
    assert!(activity
        .active_task_rows_at(UnixMillis::new(12))
        .unwrap()
        .is_empty());
}

#[test]
fn observed_settings_replace_on_stream_open_and_survive_legacy_opens() {
    let account = account("account");
    let thread = ThreadId::new("observed");
    let next = ThreadRequestSettings {
        model: Some("gpt-next".to_owned()),
        reasoning_effort: None,
        service_tier: Some("default".to_owned()),
    };
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened_observed(
            &account,
            &thread,
            UnixMillis::new(10),
            ThreadRequestSettings {
                model: Some("gpt-example".to_owned()),
                reasoning_effort: Some("high".to_owned()),
                service_tier: Some("priority".to_owned()),
            },
        )
        .unwrap();
    runtime
        .connection_opened_observed(&account, &thread, UnixMillis::new(11), next.clone())
        .unwrap();
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(12))
        .unwrap();

    assert_eq!(runtime.thread_request_settings(&thread), Some(&next));
}

#[test]
fn observed_settings_survive_an_idle_attachment() {
    let account = account("account");
    let thread = ThreadId::new("attached-observed");
    let observed = ThreadRequestSettings {
        model: Some("gpt-example".to_owned()),
        reasoning_effort: Some("high".to_owned()),
        service_tier: Some("default".to_owned()),
    };
    let mut runtime = RotationRuntime::default();
    runtime.thread_attached(&account, &thread).unwrap();
    runtime
        .connection_opened_observed(&account, &thread, UnixMillis::new(10), observed.clone())
        .unwrap();

    assert!(runtime.connection_closed(&account, &thread, UnixMillis::new(20)));
    assert_eq!(runtime.thread_request_settings(&thread), Some(&observed));
}

#[test]
fn observed_settings_round_trip_and_legacy_threads_default_them() {
    let account = account("account");
    let thread = ThreadId::new("observed");
    let observed = ThreadRequestSettings {
        model: Some("gpt-example".to_owned()),
        reasoning_effort: Some("ultra".to_owned()),
        service_tier: Some("default".to_owned()),
    };
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened_observed(&account, &thread, UnixMillis::new(10), observed.clone())
        .unwrap();

    let encoded = serde_json::to_value(&runtime).unwrap();
    let restored: RotationRuntime = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored.thread_request_settings(&thread), Some(&observed));

    let mut legacy = encoded;
    legacy["activeThreads"]["observed"]
        .as_object_mut()
        .unwrap()
        .remove("requestSettings");
    let restored: RotationRuntime = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        restored.thread_request_settings(&thread),
        Some(&ThreadRequestSettings::default())
    );
}

#[test]
fn detached_follow_up_and_idle_attachment_remain_routing_affinity() {
    let account = account("account");
    let follow_up = ThreadId::new("detached-follow-up");
    let idle = ThreadId::new("idle-attachment");
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened(&account, &follow_up, UnixMillis::new(10))
        .unwrap();
    assert!(runtime.connection_continues(&account, &follow_up, UnixMillis::new(20)));
    runtime.thread_attached(&account, &idle).unwrap();

    assert!(runtime.retained_thread_ids().contains(&follow_up));
    assert!(runtime.retained_thread_ids().contains(&idle));
}
