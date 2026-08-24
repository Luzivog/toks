use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::{
    AccountAvailability, QuotaObservation, RotationRuntime, RotationSettings, ThreadId, UnixMillis,
};

fn refresh_unknown_reset(runtime: &mut RotationRuntime, account: &AccountId, at: i64) {
    let at = UnixMillis::new(at);
    runtime.reconcile(std::slice::from_ref(account), at);
    runtime.apply_quota_observations(
        &BTreeMap::from([(account.clone(), QuotaObservation::Draining(None))]),
        at,
    );
}

#[test]
fn unknown_reset_reprobe_keeps_detached_thread_affinity_at_60_and_65_seconds() {
    let account = AccountId::new("account");
    let thread = ThreadId::new("detached-follow-up");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &thread).unwrap();

    refresh_unknown_reset(&mut runtime, &account, 0);
    runtime.thread_detached(&account, &thread);
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(60_000)),
        AccountAvailability::Draining {
            until: UnixMillis::new(60_000),
            reset_known: false,
        }
    );
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(60_000)));

    refresh_unknown_reset(&mut runtime, &account, 60_000);
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(60_000)),
        AccountAvailability::Draining {
            until: UnixMillis::new(120_000),
            reset_known: false,
        }
    );
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(60_000)));

    refresh_unknown_reset(&mut runtime, &account, 65_000);
    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(65_000)),
        AccountAvailability::Draining {
            until: UnixMillis::new(120_000),
            reset_known: false,
        }
    );
    assert!(runtime.can_drain(&account, &thread, UnixMillis::new(65_000)));
}

#[test]
fn unknown_reset_reprobe_never_admits_new_work_at_60_or_65_seconds() {
    let account = AccountId::new("account");
    let discovered = [account.clone()];
    let mut settings = RotationSettings::default();
    settings.reconcile(&discovered);
    settings.set_enabled(true);
    let mut runtime = RotationRuntime::default();

    refresh_unknown_reset(&mut runtime, &account, 0);
    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(60_000)),
        None
    );

    refresh_unknown_reset(&mut runtime, &account, 60_000);
    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(60_000)),
        None
    );

    refresh_unknown_reset(&mut runtime, &account, 65_000);
    assert_eq!(
        settings.select_account(&runtime, &discovered, UnixMillis::new(65_000)),
        None
    );
    assert!(!runtime.can_drain(
        &account,
        &ThreadId::new("new-thread"),
        UnixMillis::new(65_000)
    ));
}
