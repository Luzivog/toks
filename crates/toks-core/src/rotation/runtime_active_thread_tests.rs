use std::collections::{BTreeMap, BTreeSet};

use crate::accounts::AccountId;

use super::{
    BlockWindow, FastLimitDisposition, QuotaObservation, RotationRuntime, ThreadId, UnixMillis,
};

#[test]
fn active_count_excludes_retained_follow_up_threads() {
    let account = AccountId::new("account");
    let challenger = AccountId::new("challenger");
    let follow_up = ThreadId::new("follow-up");
    let reservation = ThreadId::new("reservation");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[account.clone(), challenger.clone()], UnixMillis::new(0));

    runtime
        .connection_opened(&account, &follow_up, UnixMillis::new(1))
        .unwrap();
    assert_eq!(runtime.in_flight_count(&account), 1);

    assert!(runtime.connection_continues(&account, &follow_up, UnixMillis::new(2)));
    runtime.reconcile(&[account.clone(), challenger.clone()], UnixMillis::new(3));
    assert_eq!(runtime.in_flight_count(&account), 0);

    let conflict = runtime
        .connection_opened(&challenger, &follow_up, UnixMillis::new(4))
        .unwrap_err();
    assert_eq!(conflict.owned_by(), &account);

    runtime
        .reserve_thread(&account, &reservation, UnixMillis::new(5))
        .unwrap();
    assert_eq!(runtime.in_flight_count(&account), 1);
    assert!(runtime.release_reservation(&account, &reservation));
    assert_eq!(runtime.in_flight_count(&account), 0);

    runtime
        .connection_opened(&account, &follow_up, UnixMillis::new(6))
        .unwrap();
    assert_eq!(runtime.in_flight_count(&account), 1);
    assert!(runtime.connection_closed(&account, &follow_up, UnixMillis::new(7)));
    assert_eq!(runtime.in_flight_count(&account), 0);
}

#[test]
fn explicit_dismissal_removes_a_dormant_follow_up_and_its_affinity() {
    let account = AccountId::new("account");
    let thread = ThreadId::new("dormant-follow-up");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .connection_opened(&account, &thread, UnixMillis::new(1))
        .unwrap();
    assert!(runtime.connection_continues(&account, &thread, UnixMillis::new(2)));
    runtime.apply_quota_observations(
        &BTreeMap::from([(
            account.clone(),
            QuotaObservation::Draining(Some(UnixMillis::new(100))),
        )]),
        UnixMillis::new(3),
    );
    runtime.fast_limit_reached(
        &account,
        &thread,
        BlockWindow::known(UnixMillis::new(100)),
        FastLimitDisposition::RetryingStandard,
        UnixMillis::new(3),
    );
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(4)));
    assert!(runtime.requires_standard_tier(&account, &thread, UnixMillis::new(4)));

    let dismissed = runtime.dismiss_cancelled_threads(&BTreeSet::from([thread.clone()]));

    assert_eq!(dismissed, BTreeSet::from([thread.clone()]));
    assert!(runtime.retained_thread_ids().is_empty());
    assert!(!runtime.can_drain(&account, &thread, UnixMillis::new(4)));
    assert!(!runtime.requires_standard_tier(&account, &thread, UnixMillis::new(4)));
}

#[test]
fn explicit_dismissal_refuses_streaming_reserved_and_attached_threads() {
    let account = AccountId::new("account");
    let streaming = ThreadId::new("streaming");
    let reserved = ThreadId::new("reserved");
    let attached = ThreadId::new("attached-follow-up");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime
        .connection_opened(&account, &streaming, UnixMillis::new(1))
        .unwrap();
    runtime
        .reserve_thread(&account, &reserved, UnixMillis::new(2))
        .unwrap();
    runtime
        .connection_opened(&account, &attached, UnixMillis::new(3))
        .unwrap();
    runtime.thread_attached(&account, &attached).unwrap();
    assert!(runtime.connection_continues(&account, &attached, UnixMillis::new(4)));

    let cancelled = BTreeSet::from([streaming.clone(), reserved.clone(), attached.clone()]);

    assert!(runtime.dismiss_cancelled_threads(&cancelled).is_empty());
    assert_eq!(
        runtime.retained_thread_ids(),
        BTreeSet::from([streaming, reserved, attached])
    );
}
