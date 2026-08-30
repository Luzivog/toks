use crate::accounts::AccountId;
use crate::codex_router::account_activation::Store;
use crate::codex_router::proxy::headers::ActivationMarker;
use crate::rotation::{ThreadId, UnixMillis};

use super::Engines;

const ATTEMPT: &str = "00000000-0000-4000-8000-000000000061";

#[tokio::test]
async fn selected_account_wins_over_higher_priority_account() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let store = Store::for_runtime(&engines.store);
    let now = UnixMillis::now().get();
    store
        .seed_running_manual_for_test(AccountId::new("b"), ATTEMPT, now)
        .unwrap();

    let selected = engines
        .first
        .select_for_activation_thread(
            &ThreadId::new("test-on-b"),
            ActivationMarker::Canonical(ATTEMPT),
        )
        .await
        .unwrap()
        .unwrap();

    assert_eq!(selected.account_id, AccountId::new("b"));
}

#[tokio::test]
async fn unavailable_selected_account_never_falls_back_to_priority_account() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let store = Store::for_runtime(&engines.store);
    let now = UnixMillis::now();
    store
        .seed_running_manual_for_test(AccountId::new("b"), ATTEMPT, now.get())
        .unwrap();
    engines
        .first
        .block_admission(
            &AccountId::new("b"),
            Some(UnixMillis::new(now.get() + 60_000)),
        )
        .unwrap();

    let selected = engines
        .first
        .select_for_activation_thread(
            &ThreadId::new("blocked-b"),
            ActivationMarker::Canonical(ATTEMPT),
        )
        .await
        .unwrap();

    assert!(selected.is_none());
    assert_eq!(
        engines
            .store
            .load()
            .unwrap()
            .in_flight_count(&AccountId::new("a")),
        0
    );
}

#[tokio::test]
async fn consumed_authorization_survives_router_restart_and_rejects_replay() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let store = Store::for_runtime(&engines.store);
    let now = UnixMillis::now().get();
    store
        .seed_running_manual_for_test(AccountId::new("b"), ATTEMPT, now)
        .unwrap();
    let thread = ThreadId::new("original-task");
    assert!(engines
        .first
        .select_for_activation_thread(&thread, ActivationMarker::Canonical(ATTEMPT))
        .await
        .unwrap()
        .is_some());

    assert!(engines
        .second
        .select_for_activation_thread(&thread, ActivationMarker::Canonical(ATTEMPT))
        .await
        .unwrap()
        .is_some());
    assert!(engines
        .second
        .select_for_activation_thread(
            &ThreadId::new("replay"),
            ActivationMarker::Canonical(ATTEMPT),
        )
        .await
        .unwrap()
        .is_none());
}
