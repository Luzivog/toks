use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::{
    AccountAvailability, BlockWindow, FastLimitDisposition, FastLimitOutcome, QuotaObservation,
    RotationRuntime, RotationRuntimeStore, ThreadId, UnixMillis,
};

fn account(id: &str) -> AccountId {
    AccountId::new(id)
}

fn draining(
    account: &AccountId,
    reset: Option<UnixMillis>,
) -> BTreeMap<AccountId, QuotaObservation> {
    BTreeMap::from([(account.clone(), QuotaObservation::Draining(reset))])
}

#[test]
fn a_usage_block_stops_only_the_thread_that_received_it() {
    let account = account("a");
    let existing = ThreadId::new("existing");
    let new_thread = ThreadId::new("new");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &existing).unwrap();
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(10),
    );

    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(20)),
        AccountAvailability::Draining {
            until: UnixMillis::new(100),
            reset_known: true,
        }
    );
    assert!(runtime.can_drain(&account, &existing, UnixMillis::new(20)));
    assert!(!runtime.can_drain(&account, &new_thread, UnixMillis::new(20)));

    runtime.thread_blocked(
        &account,
        &existing,
        BlockWindow::known(UnixMillis::new(100)),
        UnixMillis::new(21),
    );
    runtime.thread_blocked(
        &account,
        &existing,
        BlockWindow::known(UnixMillis::new(50)),
        UnixMillis::new(22),
    );
    assert_eq!(
        runtime.fast_limit_reached(
            &account,
            &existing,
            BlockWindow::known(UnixMillis::new(100)),
            FastLimitDisposition::RetryingStandard,
            UnixMillis::new(21)
        ),
        (FastLimitOutcome::AlreadyBlocked, false)
    );
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(22),
    );
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(23)),
        AccountAvailability::Blocked {
            until: UnixMillis::new(100),
            reset_known: true,
        }
    );
    assert!(!runtime.can_drain(&account, &existing, UnixMillis::new(75)));
    assert!(!runtime.can_drain(&account, &new_thread, UnixMillis::new(23)));

    let sibling = ThreadId::new("sibling");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &existing).unwrap();
    runtime.thread_attached(&account, &sibling).unwrap();
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(10),
    );
    runtime.thread_blocked(
        &account,
        &existing,
        BlockWindow::known(UnixMillis::new(100)),
        UnixMillis::new(21),
    );
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(22),
    );

    assert!(!runtime.can_drain(&account, &existing, UnixMillis::new(23)));
    assert!(runtime.can_drain(&account, &sibling, UnixMillis::new(23)));
}

#[test]
fn a_fast_limit_falls_back_only_that_thread_until_the_fast_tier_resets() {
    let account = account("a");
    let victim = ThreadId::new("victim");
    let sibling = ThreadId::new("sibling");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &victim).unwrap();
    runtime.thread_attached(&account, &sibling).unwrap();
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(10),
    );

    assert_eq!(
        runtime.fast_limit_reached(
            &account,
            &victim,
            BlockWindow::known(UnixMillis::new(50)),
            FastLimitDisposition::RetryingStandard,
            UnixMillis::new(20)
        ),
        (FastLimitOutcome::UseStandard, true)
    );
    assert_eq!(
        runtime.fast_limit_reached(
            &account,
            &victim,
            BlockWindow::known(UnixMillis::new(50)),
            FastLimitDisposition::RetryingStandard,
            UnixMillis::new(20)
        ),
        (FastLimitOutcome::UseStandard, false)
    );
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(21),
    );

    assert!(runtime.can_drain(&account, &victim, UnixMillis::new(22)));
    assert!(runtime.requires_standard_tier(&account, &victim, UnixMillis::new(22)));
    assert!(runtime.can_drain(&account, &sibling, UnixMillis::new(22)));
    assert!(!runtime.requires_standard_tier(&account, &sibling, UnixMillis::new(22)));

    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    store.save(&runtime).unwrap();
    let mut runtime = store.load().unwrap();
    assert!(runtime.requires_standard_tier(&account, &victim, UnixMillis::new(23)));
    assert!(!runtime.requires_standard_tier(&account, &sibling, UnixMillis::new(23)));
    assert!(!runtime.requires_standard_tier(&account, &victim, UnixMillis::new(50)));

    assert!(runtime.banked_reset_consumed(&account));
    assert!(!runtime.requires_standard_tier(&account, &victim, UnixMillis::new(23)));
}

#[test]
fn an_account_admission_block_keeps_existing_threads_attached() {
    let account = account("a");
    let first = ThreadId::new("first");
    let second = ThreadId::new("second");
    let fresh = ThreadId::new("fresh");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &first).unwrap();
    runtime.thread_attached(&account, &second).unwrap();

    runtime.block_admission(
        &account,
        BlockWindow::known(UnixMillis::new(100)),
        UnixMillis::new(10),
    );
    runtime.thread_blocked(
        &account,
        &first,
        BlockWindow::known(UnixMillis::new(50)),
        UnixMillis::new(11),
    );

    assert!(!runtime.can_drain(&account, &first, UnixMillis::new(20)));
    assert!(runtime.can_drain(&account, &second, UnixMillis::new(20)));
    assert!(!runtime.can_drain(&account, &fresh, UnixMillis::new(20)));
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(20)),
        AccountAvailability::Blocked {
            until: UnixMillis::new(100),
            reset_known: true,
        }
    );
}

#[test]
fn drain_expiry_clears_thread_tier_and_block_overrides() {
    let account = account("a");
    let fast_limited = ThreadId::new("fast-limited");
    let blocked = ThreadId::new("blocked");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &fast_limited).unwrap();
    runtime.thread_attached(&account, &blocked).unwrap();
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(10),
    );
    runtime.fast_limit_reached(
        &account,
        &fast_limited,
        BlockWindow::known(UnixMillis::new(100)),
        FastLimitDisposition::RetryingStandard,
        UnixMillis::new(20),
    );
    runtime.thread_blocked(
        &account,
        &blocked,
        BlockWindow::known(UnixMillis::new(100)),
        UnixMillis::new(21),
    );

    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(100));

    assert!(!runtime.requires_standard_tier(&account, &fast_limited, UnixMillis::new(100)));
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(100)),
        AccountAvailability::Available
    );
}

#[test]
fn router_restart_preserves_a_task_awaiting_follow_up() {
    let account = account("a");
    let thread = ThreadId::new("tool-follow-up");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(10))
        .unwrap();
    assert!(runtime.connection_continues(&account, &thread, UnixMillis::new(20)));

    runtime.reset_connections(UnixMillis::new(30));
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(31),
    );

    assert_eq!(runtime.active_threads(&account), 1);
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(32)));
}

#[test]
fn a_pending_duplicate_reservation_survives_the_first_stream_closing() {
    let account = account("a");
    let thread = ThreadId::new("duplicate");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .reserve_thread(&account, &thread, UnixMillis::new(10))
        .unwrap();
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(11))
        .unwrap();
    runtime
        .reserve_thread(&account, &thread, UnixMillis::new(12))
        .unwrap();

    assert!(runtime.connection_closed(&account, &thread, UnixMillis::new(13)));
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(14),
    );

    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(15)));
}

#[test]
fn releasing_a_reservation_cannot_delete_a_worker_owned_live_stream() {
    let account = account("a");
    let thread = ThreadId::new("worker-owned-with-reservation");
    let owner = super::WorkerConnectionOwner::new(9, 901).unwrap();
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .connection_opened_by(owner, &account, &thread, UnixMillis::new(10))
        .unwrap();
    runtime
        .reserve_thread(&account, &thread, UnixMillis::new(11))
        .unwrap();

    assert!(runtime.release_reservation(&account, &thread));
    assert_eq!(runtime.active_threads(&account), 1);
    assert!(runtime.connection_closed_by(owner, &account, &thread, UnixMillis::new(12)));
    assert_eq!(runtime.active_threads(&account), 0);
}

#[test]
fn a_live_thread_cannot_be_reassigned_to_another_account() {
    let a = account("a");
    let b = account("b");
    let thread = ThreadId::new("cross-account-overlap");
    let first = super::WorkerConnectionOwner::new(1, 101).unwrap();
    let second = super::WorkerConnectionOwner::new(2, 201).unwrap();
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[a.clone(), b.clone()], UnixMillis::new(0));
    runtime
        .connection_opened_by(first, &a, &thread, UnixMillis::new(1))
        .unwrap();
    runtime.thread_attached_by(first, &a, &thread).unwrap();
    let owned_by_a = runtime.clone();

    let reservation = runtime
        .reserve_thread(&b, &thread, UnixMillis::new(2))
        .unwrap_err();
    assert_eq!(reservation.requested(), &b);
    assert_eq!(reservation.owned_by(), &a);
    assert_eq!(runtime, owned_by_a);

    let stream = runtime
        .connection_opened_by(second, &b, &thread, UnixMillis::new(3))
        .unwrap_err();
    assert_eq!(stream.owned_by(), &a);
    assert_eq!(runtime, owned_by_a);

    let attachment = runtime.thread_attached_by(second, &b, &thread).unwrap_err();
    assert_eq!(attachment.owned_by(), &a);
    assert_eq!(runtime, owned_by_a);

    assert!(runtime.connection_closed_by(first, &a, &thread, UnixMillis::new(4)));
    assert!(runtime
        .reserve_thread(&b, &thread, UnixMillis::new(5))
        .is_err());
    assert!(runtime.thread_detached_by(first, &a, &thread));

    runtime
        .reserve_thread(&b, &thread, UnixMillis::new(6))
        .unwrap();
    runtime
        .connection_opened_by(second, &b, &thread, UnixMillis::new(7))
        .unwrap();
    runtime.thread_attached_by(second, &b, &thread).unwrap();
    assert_eq!(runtime.active_threads(&a), 0);
    assert_eq!(runtime.active_threads(&b), 1);
}

#[test]
fn websocket_detach_cannot_erase_an_overlapping_http_stream() {
    let account = account("a");
    let thread = ThreadId::new("mixed-transport");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(10))
        .unwrap();
    runtime.thread_attached(&account, &thread).unwrap();

    assert!(runtime.thread_detached(&account, &thread));
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(11),
    );

    assert_eq!(runtime.active_threads(&account), 1);
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(12)));
}

#[test]
fn an_abandoned_selection_reservation_expires() {
    let account = account("a");
    let thread = ThreadId::new("cancelled-selection");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .reserve_thread(&account, &thread, UnixMillis::new(1))
        .unwrap();
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(1_000_000))),
        UnixMillis::new(2),
    );
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(3)));

    runtime.reconcile(
        std::slice::from_ref(&account),
        UnixMillis::new(5 * 60 * 1_000 + 1),
    );

    assert_eq!(runtime.active_threads(&account), 0);
    assert!(!runtime.can_drain(&account, &thread, UnixMillis::new(5 * 60 * 1_000 + 1)));
}

#[test]
fn legacy_connection_fields_load_and_clear_without_losing_follow_up_state() {
    let account = account("a");
    let mut value = serde_json::to_value(RotationRuntime::default()).unwrap();
    value["accounts"]["a"] = serde_json::to_value(super::AccountRuntime::default()).unwrap();
    value["activeThreads"] = serde_json::json!({
        "legacy-live": {
            "accountId": "a",
            "streams": 1,
            "reservations": 0,
            "awaitingFollowUp": false,
            "lastActivityAt": 10
        },
        "legacy-follow-up": {
            "accountId": "a",
            "streams": 1,
            "reservations": 0,
            "awaitingFollowUp": true,
            "lastActivityAt": 11
        }
    });
    value["attachedThreads"] = serde_json::json!({
        "legacy-websocket": { "account": "a", "connections": 1 }
    });
    let mut runtime: RotationRuntime = serde_json::from_value(value).unwrap();

    assert!(runtime.reconcile_connection_owners(&Default::default()));
    assert_eq!(runtime.active_threads(&account), 1);
    runtime.apply_quota_observations(
        &draining(&account, Some(UnixMillis::new(100))),
        UnixMillis::new(20),
    );
    assert!(runtime.can_drain(
        &account,
        &ThreadId::new("legacy-follow-up"),
        UnixMillis::new(21)
    ));
    assert!(!runtime.can_drain(&account, &ThreadId::new("legacy-live"), UnixMillis::new(21)));
    assert!(!runtime.can_drain(
        &account,
        &ThreadId::new("legacy-websocket"),
        UnixMillis::new(21)
    ));

    let encoded = serde_json::to_value(runtime).unwrap();
    assert_eq!(encoded["activeThreads"]["legacy-follow-up"]["streams"], 0);
    assert_eq!(
        encoded["activeThreads"]["legacy-follow-up"]["streamOwners"],
        serde_json::json!({})
    );
}
