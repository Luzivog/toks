use crate::accounts::AccountId;

use super::{RotationRuntime, ThreadId, UnixMillis};

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
    assert_eq!(runtime.active_threads(&account), 1);

    assert!(runtime.connection_continues(&account, &follow_up, UnixMillis::new(2)));
    runtime.reconcile(&[account.clone(), challenger.clone()], UnixMillis::new(3));
    assert_eq!(runtime.active_threads(&account), 0);

    let conflict = runtime
        .connection_opened(&challenger, &follow_up, UnixMillis::new(4))
        .unwrap_err();
    assert_eq!(conflict.owned_by(), &account);

    runtime
        .reserve_thread(&account, &reservation, UnixMillis::new(5))
        .unwrap();
    assert_eq!(runtime.active_threads(&account), 1);
    assert!(runtime.release_reservation(&account, &reservation));
    assert_eq!(runtime.active_threads(&account), 0);

    runtime
        .connection_opened(&account, &follow_up, UnixMillis::new(6))
        .unwrap();
    assert_eq!(runtime.active_threads(&account), 1);
    assert!(runtime.connection_closed(&account, &follow_up, UnixMillis::new(7)));
    assert_eq!(runtime.active_threads(&account), 0);
}
