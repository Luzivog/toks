use crate::accounts::AccountId;
use crate::rotation::ThreadId;

use super::super::model::{FailureReason, TASK_TIMEOUT_MS};
use super::super::route_authorization::RouteClaim;
use super::super::status::ManualTestOutcome;
use super::super::store::Store;

const NOW: i64 = 1_800_000_000_000;
const ATTEMPT: &str = "00000000-0000-4000-8000-000000000051";

#[test]
fn one_shot_authorization_binds_the_selected_account_to_one_task() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("activation.json");
    let store = Store::at(path.clone());
    store
        .seed_running_manual_for_test(AccountId::new("b"), ATTEMPT, NOW)
        .unwrap();
    let thread = ThreadId::new("task-b");

    assert_eq!(
        store.claim_route(ATTEMPT, &thread, NOW + 1).unwrap(),
        RouteClaim::Selected(AccountId::new("b"))
    );
    assert_eq!(
        Store::at(path)
            .claim_route(ATTEMPT, &thread, NOW + 2)
            .unwrap(),
        RouteClaim::Selected(AccountId::new("b"))
    );
    assert_eq!(
        store
            .claim_route(ATTEMPT, &ThreadId::new("replay"), NOW + 2)
            .unwrap(),
        RouteClaim::Denied
    );
}

#[test]
fn expired_authorization_fails_closed_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("activation.json");
    Store::at(path.clone())
        .seed_running_manual_for_test(AccountId::new("b"), ATTEMPT, NOW)
        .unwrap();

    assert_eq!(
        Store::at(path)
            .claim_route(ATTEMPT, &ThreadId::new("too-late"), NOW + TASK_TIMEOUT_MS,)
            .unwrap(),
        RouteClaim::Denied
    );
}

#[test]
fn successful_receipt_requires_the_router_to_observe_the_requested_account() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("activation.json"));
    let requested = AccountId::new("b");
    let thread = ThreadId::new("task-b");
    store
        .seed_running_manual_for_test(requested.clone(), ATTEMPT, NOW)
        .unwrap();
    assert_eq!(
        store.claim_route(ATTEMPT, &thread, NOW + 1).unwrap(),
        RouteClaim::Selected(requested.clone())
    );
    store
        .observe_route(ATTEMPT, &thread, &requested, NOW + 2)
        .unwrap();
    store.finish_for_test(ATTEMPT, Ok(()), NOW + 3).unwrap();

    let receipt = store
        .status_for_test(&requested, NOW + 4)
        .unwrap()
        .manual_receipt
        .unwrap();
    assert_eq!(receipt.requested_account, requested);
    assert_eq!(receipt.observed_account, Some(AccountId::new("b")));
    assert_eq!(receipt.thread_id, Some(thread));
    assert_eq!(receipt.started_at_ms, NOW);
    assert_eq!(receipt.routed_at_ms, Some(NOW + 2));
    assert_eq!(receipt.completed_at_ms, NOW + 3);
    assert_eq!(receipt.outcome, ManualTestOutcome::Succeeded);
}

#[test]
fn process_success_without_an_authoritative_route_is_recorded_as_failure() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("activation.json"));
    let account = AccountId::new("b");
    store
        .seed_running_manual_for_test(account.clone(), ATTEMPT, NOW)
        .unwrap();

    store.finish_for_test(ATTEMPT, Ok(()), NOW + 1).unwrap();

    let receipt = store
        .status_for_test(&account, NOW + 2)
        .unwrap()
        .manual_receipt
        .unwrap();
    assert_eq!(receipt.outcome, ManualTestOutcome::Failed);
    assert_eq!(receipt.observed_account, None);
    assert_eq!(receipt.thread_id, None);
}

#[test]
fn unavailable_target_receipt_keeps_the_bound_task_identity() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("activation.json"));
    let account = AccountId::new("b");
    let thread = ThreadId::new("unavailable-b");
    store
        .seed_running_manual_for_test(account.clone(), ATTEMPT, NOW)
        .unwrap();
    assert_eq!(
        store.claim_route(ATTEMPT, &thread, NOW + 1).unwrap(),
        RouteClaim::Selected(account.clone())
    );
    store
        .finish_for_test(ATTEMPT, Err(FailureReason::Unsuccessful), NOW + 2)
        .unwrap();

    let receipt = store
        .status_for_test(&account, NOW + 3)
        .unwrap()
        .manual_receipt
        .unwrap();
    assert_eq!(receipt.thread_id, Some(thread));
    assert_eq!(receipt.observed_account, None);
    assert_eq!(receipt.outcome, ManualTestOutcome::Failed);
}

#[test]
fn timeout_keeps_the_router_observation_in_a_failed_receipt() {
    let directory = tempfile::tempdir().unwrap();
    let store = Store::at(directory.path().join("activation.json"));
    let account = AccountId::new("b");
    let thread = ThreadId::new("timed-out-b");
    store
        .seed_running_manual_for_test(account.clone(), ATTEMPT, NOW)
        .unwrap();
    assert_eq!(
        store.claim_route(ATTEMPT, &thread, NOW + 1).unwrap(),
        RouteClaim::Selected(account.clone())
    );
    store
        .observe_route(ATTEMPT, &thread, &account, NOW + 2)
        .unwrap();

    let receipt = store
        .status_for_test(&account, NOW + TASK_TIMEOUT_MS)
        .unwrap()
        .manual_receipt
        .unwrap();

    assert_eq!(receipt.observed_account, Some(account));
    assert_eq!(receipt.thread_id, Some(thread));
    assert_eq!(receipt.outcome, ManualTestOutcome::Failed);
}
