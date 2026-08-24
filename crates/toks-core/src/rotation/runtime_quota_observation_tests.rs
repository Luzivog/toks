use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::{
    AccountAvailability, BlockWindow, FastLimitDisposition, QuotaObservation, RotationRuntime,
    RotationRuntimeStore, ThreadId, UnixMillis,
};

#[test]
fn unknown_quota_observation_preserves_drain_affinity_and_standard_override() {
    let account = AccountId::new("draining");
    let existing = ThreadId::new("existing");
    let fresh = ThreadId::new("fresh");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &existing).unwrap();
    runtime.apply_quota_observations(
        &BTreeMap::from([(
            account.clone(),
            QuotaObservation::Draining(Some(UnixMillis::new(100))),
        )]),
        UnixMillis::new(10),
    );
    runtime.fast_limit_reached(
        &account,
        &existing,
        BlockWindow::known(UnixMillis::new(100)),
        FastLimitDisposition::RetryingStandard,
        UnixMillis::new(11),
    );

    runtime.apply_quota_observations(
        &BTreeMap::from([(account.clone(), QuotaObservation::Unknown)]),
        UnixMillis::new(20),
    );

    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(20)),
        AccountAvailability::Draining {
            until: UnixMillis::new(100),
            reset_known: true,
        }
    );
    assert!(runtime.can_drain(&account, &existing, UnixMillis::new(20)));
    assert!(runtime.requires_standard_tier(&account, &existing, UnixMillis::new(20)));
    assert!(!runtime.can_drain(&account, &fresh, UnixMillis::new(20)));
}

#[test]
fn discovery_omission_and_restart_preserve_drain_until_rediscovery() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let account = AccountId::new("temporarily-omitted");
    let existing = ThreadId::new("existing");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.thread_attached(&account, &existing).unwrap();
    runtime.apply_quota_observations(
        &BTreeMap::from([(
            account.clone(),
            QuotaObservation::Draining(Some(UnixMillis::new(100))),
        )]),
        UnixMillis::new(10),
    );

    runtime.reconcile(&[], UnixMillis::new(20));
    store.save(&runtime).unwrap();
    let mut restarted = store.load().unwrap();

    assert_eq!(
        restarted.accounts()[&account].availability(UnixMillis::new(20)),
        AccountAvailability::Draining {
            until: UnixMillis::new(100),
            reset_known: true,
        }
    );
    restarted.reconcile(std::slice::from_ref(&account), UnixMillis::new(30));
    assert!(restarted.can_drain(&account, &existing, UnixMillis::new(30)));
    assert!(!restarted.can_drain(&account, &ThreadId::new("fresh"), UnixMillis::new(30)));
}

#[test]
fn discovery_omission_and_restart_preserve_confirmed_hard_block() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let account = AccountId::new("temporarily-omitted");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.block_admission(
        &account,
        BlockWindow::known(UnixMillis::new(100)),
        UnixMillis::new(10),
    );

    runtime.reconcile(&[], UnixMillis::new(20));
    store.save(&runtime).unwrap();
    let mut restarted = store.load().unwrap();

    assert_eq!(
        restarted.accounts()[&account].availability(UnixMillis::new(20)),
        AccountAvailability::Blocked {
            until: UnixMillis::new(100),
            reset_known: true,
        }
    );
    restarted.reconcile(std::slice::from_ref(&account), UnixMillis::new(30));
    assert!(!restarted.is_available(&account, UnixMillis::new(30)));
}

#[test]
fn authoritative_available_observation_clears_a_confirmed_hard_block() {
    let account = AccountId::new("refreshed-after-block");
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.block_admission(
        &account,
        BlockWindow::known(UnixMillis::new(100)),
        UnixMillis::new(10),
    );

    runtime.apply_quota_observations(
        &BTreeMap::from([(account.clone(), QuotaObservation::ObservedAvailable)]),
        UnixMillis::new(20),
    );

    assert_eq!(
        runtime.accounts()[&account].availability(UnixMillis::new(20)),
        AccountAvailability::Available
    );
}

#[test]
fn discovery_omission_and_restart_preserve_rejected_credential_history_after_repair() {
    let directory = tempfile::tempdir().unwrap();
    let store = RotationRuntimeStore::for_data_dir(directory.path());
    let account = AccountId::new("temporarily-omitted");
    let rejected = "credential-a";
    let repaired = "credential-b";
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(&account), UnixMillis::new(0));
    runtime.auth_failed_for_credential(&account, UnixMillis::new(10), Some(rejected));
    let failure = runtime.auth_failure(&account).unwrap();
    assert!(runtime.sign_in_restored_by_proof(&account, failure, repaired));

    runtime.reconcile(&[], UnixMillis::new(20));
    store.save(&runtime).unwrap();
    let mut restarted = store.load().unwrap();

    assert!(restarted.credential_was_rejected(&account, rejected));
    restarted.reconcile(std::slice::from_ref(&account), UnixMillis::new(30));
    assert!(restarted.credential_was_rejected(&account, rejected));
}
