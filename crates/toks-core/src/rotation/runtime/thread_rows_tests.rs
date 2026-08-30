use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, ThreadRequestSettings, UnixMillis};

fn account(id: &str) -> AccountId {
    AccountId::new(id)
}

#[test]
fn live_thread_rows_include_streams_and_attached_follow_ups_in_start_order() {
    let account = account("account");
    let streaming = ThreadId::new("streaming");
    let attached_follow_up = ThreadId::new("attached-follow-up");
    let reservation = ThreadId::new("reservation");
    let attached_idle = ThreadId::new("attached-idle");
    let mut runtime = RotationRuntime::default();

    runtime
        .connection_opened(&account, &streaming, UnixMillis::new(35))
        .unwrap();
    runtime
        .connection_opened(&account, &streaming, UnixMillis::new(40))
        .unwrap();
    runtime
        .reserve_thread(&account, &reservation, UnixMillis::new(30))
        .unwrap();
    runtime
        .connection_opened(&account, &attached_follow_up, UnixMillis::new(10))
        .unwrap();
    runtime
        .thread_attached(&account, &attached_follow_up)
        .unwrap();
    assert!(runtime.connection_continues(&account, &attached_follow_up, UnixMillis::new(20)));
    runtime.thread_attached(&account, &attached_idle).unwrap();

    let rows = runtime.live_thread_rows();
    let ids = rows
        .iter()
        .map(|row| row.thread_id.as_str())
        .collect::<Vec<_>>();

    assert_eq!(ids, ["streaming", "attached-follow-up"]);
    assert_eq!(rows[0].account_id, account);
}

#[test]
fn live_thread_count_matches_rows_for_attached_follow_ups() {
    let other = account("other");
    let account = account("account");
    let streaming = ThreadId::new("streaming");
    let attached_follow_up = ThreadId::new("attached-follow-up");
    let detached_follow_up = ThreadId::new("detached-follow-up");
    let reservation = ThreadId::new("reservation");
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened(&account, &streaming, UnixMillis::new(1))
        .unwrap();
    for thread in [&attached_follow_up, &detached_follow_up] {
        runtime
            .connection_opened(&account, thread, UnixMillis::new(2))
            .unwrap();
    }
    runtime
        .thread_attached(&account, &attached_follow_up)
        .unwrap();
    assert!(runtime.connection_continues(&account, &attached_follow_up, UnixMillis::new(3)));
    assert!(runtime.connection_continues(&account, &detached_follow_up, UnixMillis::new(3)));
    runtime
        .reserve_thread(&account, &reservation, UnixMillis::new(4))
        .unwrap();

    assert_eq!(runtime.live_thread_count(&account), 2);
    assert_eq!(runtime.live_thread_count(&other), 0);
    assert_eq!(
        runtime
            .live_thread_rows()
            .iter()
            .filter(|row| row.account_id == account)
            .count(),
        2
    );
}

#[test]
fn live_thread_rows_keep_start_order_when_an_older_thread_receives_new_activity() {
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
        .live_thread_rows()
        .into_iter()
        .map(|row| row.thread_id)
        .collect::<Vec<_>>();

    runtime
        .connection_opened(&account, &older, UnixMillis::new(30))
        .unwrap();
    let after = runtime
        .live_thread_rows()
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
        .connection_opened_observed(&account, &thread, UnixMillis::new(10), observed)
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

    assert_eq!(
        runtime.live_thread_rows()[0].request_settings,
        next_observation
    );
}

#[test]
fn observed_settings_survive_an_invisible_idle_attachment() {
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
    assert!(runtime.live_thread_rows().is_empty());
    assert_eq!(runtime.thread_request_settings(&thread), Some(&observed));
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
    let reencoded = serde_json::to_value(restored).unwrap();
    assert!(reencoded["activeThreads"]["observed"]
        .get("requestSettings")
        .is_none());
}

#[test]
fn detached_follow_up_affinity_is_retained_but_not_a_live_thread_row() {
    let owner = account("account");
    let challenger = account("other");
    let thread = ThreadId::new("detached-follow-up");
    let mut runtime = RotationRuntime::default();
    runtime
        .connection_opened(&owner, &thread, UnixMillis::new(10))
        .unwrap();
    assert!(runtime.connection_continues(&owner, &thread, UnixMillis::new(20)));

    assert!(runtime.live_thread_rows().is_empty());
    assert!(runtime.retained_thread_ids().contains(&thread));
    assert_eq!(
        runtime
            .connection_opened(&challenger, &thread, UnixMillis::new(30))
            .unwrap_err()
            .owned_by(),
        &owner
    );
}

#[test]
fn idle_attachment_is_retained_but_not_a_live_thread_row() {
    let owner = account("account");
    let thread = ThreadId::new("idle-attachment");
    let mut runtime = RotationRuntime::default();
    runtime.thread_attached(&owner, &thread).unwrap();

    assert!(runtime.live_thread_rows().is_empty());
    assert!(runtime.retained_thread_ids().contains(&thread));
}
