use super::Engines;
use crate::accounts::AccountId;
use crate::rotation::{QuotaObservation, ResumeAuthorization, ThreadId, UnixMillis};
use crate::storage::StoreUpdate;

const STALE_ATTEMPT: &str = "00000000-0000-4000-8000-000000000003";
const CURRENT_ATTEMPT: &str = "00000000-0000-4000-8000-000000000004";

#[test]
fn revalidates_the_selected_account_without_losing_the_waiting_entry() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let thread = ThreadId::new("stale-selection");
    let account_a = AccountId::new("a");
    let account_b = AccountId::new("b");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();
    assert_eq!(
        engines.first.eligible_account().unwrap(),
        Some(account_a.clone())
    );
    engines
        .first
        .block_admission(
            &account_a,
            Some(UnixMillis::new(
                chrono::Utc::now().timestamp_millis() + 60_000,
            )),
        )
        .unwrap();

    assert_eq!(
        engines
            .second
            .authorize_resume(&waiting, STALE_ATTEMPT, &account_a)
            .unwrap(),
        ResumeAuthorization::Stale
    );
    assert_eq!(
        engines.store.load().unwrap().waiting_threads(),
        std::slice::from_ref(&waiting)
    );
    assert_eq!(
        engines.second.eligible_account().unwrap(),
        Some(account_b.clone())
    );
    assert_eq!(
        engines
            .second
            .authorize_resume(&waiting, CURRENT_ATTEMPT, &account_b)
            .unwrap(),
        ResumeAuthorization::Acquired
    );
}

#[test]
fn priority_change_before_authorization_selects_the_new_current_account() {
    let engines = Engines::with_accounts(&["a", "b"]);
    let thread = ThreadId::new("stale-priority");
    let account_a = AccountId::new("a");
    let account_b = AccountId::new("b");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();
    assert_eq!(
        engines.first.eligible_account().unwrap(),
        Some(account_a.clone())
    );
    engines.prioritize(&account_b);

    assert_eq!(
        engines
            .first
            .authorize_resume(&waiting, STALE_ATTEMPT, &account_a)
            .unwrap(),
        ResumeAuthorization::Stale
    );
    assert_eq!(
        engines.store.load().unwrap().waiting_threads(),
        std::slice::from_ref(&waiting)
    );
    assert_eq!(engines.first.eligible_account().unwrap(), Some(account_b));
}

#[test]
fn cancellation_before_authorization_preserves_the_exact_waiting_entry() {
    let engines = Engines::new();
    let thread = ThreadId::new("cancelled-selection");
    let account = AccountId::new("a");
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();
    assert_eq!(
        engines.first.eligible_account().unwrap(),
        Some(account.clone())
    );
    engines.cancel(&thread);

    assert_eq!(
        engines
            .second
            .authorize_resume(&waiting, CURRENT_ATTEMPT, &account)
            .unwrap(),
        ResumeAuthorization::Cancelled
    );
    assert_eq!(
        engines.store.load().unwrap().waiting_threads(),
        std::slice::from_ref(&waiting)
    );
}

#[tokio::test]
async fn exact_resume_admission_routes_the_grandfathered_thread_on_its_draining_account() {
    let engines = Engines::new();
    let thread = ThreadId::new("grandfathered-resume");
    let account = AccountId::new("a");
    assert!(engines.first.route(&account, &thread).unwrap().is_some());
    engines.first.continue_response(&account, &thread).unwrap();
    engines
        .first
        .runtime
        .update(|runtime| {
            let changed = runtime.apply_quota_observations(
                &std::collections::BTreeMap::from([(
                    account.clone(),
                    QuotaObservation::Draining(Some(UnixMillis::new(i64::MAX))),
                )]),
                UnixMillis::new(10),
            );
            StoreUpdate::from_changed((), changed)
        })
        .unwrap();
    engines.first.waiting(&thread).unwrap();
    let waiting = engines.store.load().unwrap().waiting_threads()[0].clone();

    assert_eq!(
        engines
            .first
            .authorize_resume(&waiting, CURRENT_ATTEMPT, &account)
            .unwrap(),
        ResumeAuthorization::Acquired
    );
    let threadless = engines
        .second
        .select_for_thread_authorized(
            None,
            crate::codex_router::proxy::headers::ResumeMarker::Canonical(CURRENT_ATTEMPT),
            &Default::default(),
        )
        .await
        .unwrap();
    assert!(matches!(
        threadless,
        super::super::selection::RouteSelection::Selected(selected)
            if selected.account_id == account
    ));
    assert_eq!(
        engines
            .second
            .route_authorized(&account, &thread, Some(CURRENT_ATTEMPT))
            .unwrap(),
        Some(super::super::RouteTier::Fast)
    );
}
