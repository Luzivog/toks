use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, ThreadRequestSettings, ThreadStatus, UnixMillis};

fn account(id: &str) -> AccountId {
    AccountId::new(id)
}

#[test]
fn thread_rows_compose_active_and_attached_threads_in_start_order() {
    let account = account("account");
    let streaming = ThreadId::new("streaming");
    let pending_a = ThreadId::new("pending-a");
    let pending_b = ThreadId::new("pending-b");
    let follow_up = ThreadId::new("follow-up");
    let attached_a = ThreadId::new("attached-a");
    let attached_b = ThreadId::new("attached-b");
    let mut runtime = RotationRuntime::default();

    runtime
        .connection_opened(&account, &streaming, UnixMillis::new(35))
        .unwrap();
    runtime
        .connection_opened(&account, &streaming, UnixMillis::new(40))
        .unwrap();
    runtime.thread_attached(&account, &streaming).unwrap();
    runtime
        .reserve_thread(&account, &pending_b, UnixMillis::new(30))
        .unwrap();
    runtime
        .reserve_thread(&account, &pending_a, UnixMillis::new(30))
        .unwrap();
    runtime
        .connection_opened(&account, &follow_up, UnixMillis::new(10))
        .unwrap();
    assert!(runtime.connection_continues(&account, &follow_up, UnixMillis::new(20)));
    runtime.thread_attached(&account, &attached_b).unwrap();
    runtime.thread_attached(&account, &attached_a).unwrap();

    let rows = runtime.thread_rows();
    let ids = rows
        .iter()
        .map(|row| row.thread_id.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        [
            "streaming",
            "pending-a",
            "pending-b",
            "follow-up",
            "attached-a",
            "attached-b"
        ]
    );
    assert_eq!(rows[0].status, ThreadStatus::Streaming { stream_count: 2 });
    assert_eq!(rows[0].started_at, Some(UnixMillis::new(35)));
    assert_eq!(rows[0].last_activity_at, Some(UnixMillis::new(40)));
    assert_eq!(rows[1].status, ThreadStatus::ReservationPending);
    assert_eq!(rows[3].status, ThreadStatus::AwaitingFollowUp);
    assert_eq!(rows[4].status, ThreadStatus::AttachedIdle);
    assert_eq!(rows[4].account_id.as_ref(), Some(&account));
    assert_eq!(rows[4].started_at, None);
    assert_eq!(rows[4].last_activity_at, None);
}

#[test]
fn thread_rows_keep_their_order_when_an_older_thread_receives_new_activity() {
    let account = account("account");
    let older = ThreadId::new("older");
    let newer = ThreadId::new("newer");
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened(&account, &older, UnixMillis::new(10))
        .unwrap();
    runtime
        .connection_opened(&account, &newer, UnixMillis::new(20))
        .unwrap();
    let before = runtime
        .thread_rows()
        .into_iter()
        .map(|row| row.thread_id)
        .collect::<Vec<_>>();

    runtime
        .connection_opened(&account, &older, UnixMillis::new(30))
        .unwrap();
    let after = runtime
        .thread_rows()
        .into_iter()
        .map(|row| row.thread_id)
        .collect::<Vec<_>>();

    assert_eq!(before, [newer, older]);
    assert_eq!(after, before);
}

#[test]
fn observed_settings_replace_on_stream_open_and_survive_legacy_opens() {
    let account = account("account");
    let thread = ThreadId::new("observed");
    let observed = ThreadRequestSettings {
        model: Some("gpt-example".to_owned()),
        reasoning_effort: Some("high".to_owned()),
        service_tier: Some("priority".to_owned()),
    };
    let next_observation = ThreadRequestSettings {
        model: Some("gpt-next".to_owned()),
        reasoning_effort: None,
        service_tier: Some("default".to_owned()),
    };
    let mut runtime = RotationRuntime::default();

    runtime
        .connection_opened_observed(&account, &thread, UnixMillis::new(10), observed.clone())
        .unwrap();
    runtime
        .connection_opened_observed(
            &account,
            &thread,
            UnixMillis::new(11),
            next_observation.clone(),
        )
        .unwrap();
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(12))
        .unwrap();

    assert_eq!(runtime.thread_rows()[0].request_settings, next_observation);
}

#[test]
fn attached_idle_rows_keep_the_last_observed_settings_until_detach() {
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
    let row = &runtime.thread_rows()[0];
    assert_eq!(row.status, ThreadStatus::AttachedIdle);
    assert_eq!(row.last_activity_at, Some(UnixMillis::new(20)));
    assert_eq!(row.request_settings, observed);

    assert!(runtime.thread_detached(&account, &thread));
    assert!(runtime.thread_rows().is_empty());
}

#[test]
fn observed_settings_round_trip_and_legacy_active_threads_default_them() {
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
    assert_eq!(
        encoded["activeThreads"]["observed"]["requestSettings"],
        serde_json::json!({
            "model": "gpt-example",
            "reasoningEffort": "ultra",
            "serviceTier": "default"
        })
    );
    let restored: RotationRuntime = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(restored.thread_rows()[0].request_settings, observed);

    let mut legacy = encoded;
    legacy["activeThreads"]["observed"]
        .as_object_mut()
        .unwrap()
        .remove("requestSettings");
    let restored: RotationRuntime = serde_json::from_value(legacy).unwrap();
    assert_eq!(
        restored.thread_rows()[0].request_settings,
        ThreadRequestSettings::default()
    );
    let reencoded = serde_json::to_value(restored).unwrap();
    assert!(reencoded["activeThreads"]["observed"]
        .get("requestSettings")
        .is_none());
}
