use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, UnixMillis};

#[test]
fn account_reconciliation_cannot_skip_expired_reservations() {
    let original = AccountId::new("original");
    let challenger = AccountId::new("challenger");
    let newly_discovered = AccountId::new("newly-discovered");
    let thread = ThreadId::new("expired-reservation");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[original.clone(), challenger.clone()], UnixMillis::new(0));
    runtime
        .reserve_thread(&original, &thread, UnixMillis::new(1))
        .unwrap();

    runtime.reconcile(
        &[original, challenger.clone(), newly_discovered],
        UnixMillis::new(5 * 60 * 1_000 + 2),
    );

    runtime
        .reserve_thread(&challenger, &thread, UnixMillis::new(5 * 60 * 1_000 + 3))
        .unwrap();
}

#[test]
fn undiscovered_reservation_does_not_age_out_before_its_owner_releases_it() {
    let original = AccountId::new("original");
    let challenger = AccountId::new("challenger");
    let thread = ThreadId::new("long-credential-read");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[original.clone(), challenger.clone()], UnixMillis::new(0));
    runtime
        .reserve_thread(&original, &thread, UnixMillis::new(1))
        .unwrap();

    runtime.reconcile(
        std::slice::from_ref(&challenger),
        UnixMillis::new(7 * 24 * 60 * 60 * 1_000),
    );

    assert!(runtime
        .reserve_thread(&challenger, &thread, UnixMillis::new(i64::MAX))
        .is_err());
    assert!(runtime.release_reservation(&original, &thread));
    runtime
        .reserve_thread(&challenger, &thread, UnixMillis::new(i64::MAX))
        .unwrap();
}

#[test]
fn undiscovered_follow_up_does_not_age_out_before_its_final_response() {
    let original = AccountId::new("original");
    let challenger = AccountId::new("challenger");
    let thread = ThreadId::new("long-tool-call");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(&[original.clone(), challenger.clone()], UnixMillis::new(0));
    runtime
        .connection_opened(&original, &thread, UnixMillis::new(1))
        .unwrap();
    assert!(runtime.connection_continues(&original, &thread, UnixMillis::new(2)));

    runtime.reconcile(
        std::slice::from_ref(&challenger),
        UnixMillis::new(7 * 24 * 60 * 60 * 1_000),
    );

    assert!(runtime
        .connection_opened(&challenger, &thread, UnixMillis::new(i64::MAX))
        .is_err());
    runtime
        .connection_opened(&original, &thread, UnixMillis::new(i64::MAX - 1))
        .unwrap();
    assert!(runtime.connection_closed(&original, &thread, UnixMillis::new(i64::MAX)));
    runtime
        .connection_opened(&challenger, &thread, UnixMillis::new(i64::MAX))
        .unwrap();
}
