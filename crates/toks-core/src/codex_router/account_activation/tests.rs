use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use chrono::{TimeZone as _, Utc};

use crate::accounts::{
    AccountId, AccountIdentityKind, AccountProfile, AccountSource, CodexAuthSnapshot,
    CredentialProfileId, CredentialProfileKind, ProviderAccount, ProviderLimitCollection,
};
use crate::limits::{LimitSnapshot, LimitWindow, Provider, SnapshotFreshness, SnapshotStatus};
use crate::storage::StoreUpdate;

use super::authority::{Authority, WeeklyUsage};
use super::model::{Document, FailureReason, JobPhase, PROVISIONAL_WEEK_MS};
use super::owner::ProcessOwner;
use super::{planner, requests, AutomaticTestStatus, ManualRequest, ManualTestStatus};

const NOW: i64 = 1_800_000_000_000;
const WEEK: i64 = 7 * 24 * 60 * 60 * 1_000;

mod command;

#[test]
fn only_unused_proved_weekly_accounts_are_claimed() {
    let mut document = Document::new();
    let authorities = [
        authority("a", 0.0, NOW + WEEK),
        authority("b", 0.0, NOW + WEEK),
        authority("c", 5.0, NOW + WEEK),
    ];
    let launches = planner::observe(&mut document, &authorities, NOW);
    assert_eq!(launches.len(), 2);
    assert_eq!(
        document.accounts[&AccountId::new("c")].active_until_ms,
        Some(NOW + WEEK)
    );
}

#[test]
fn rolling_unused_reset_timestamp_does_not_duplicate_a_cycle() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    let first = planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW);
    assert_eq!(first.len(), 1);
    assert!(planner::finish(
        &mut document,
        &first[0].id,
        true,
        FailureReason::Unsuccessful,
        NOW + 20
    ));
    let provisional = NOW + 20 + PROVISIONAL_WEEK_MS;
    assert!(planner::observe(
        &mut document,
        &[authority("a", 0.0, NOW + WEEK + 60_000)],
        NOW + 60_000
    )
    .is_empty());
    assert_eq!(
        document.accounts[&account].active_until_ms,
        Some(provisional)
    );
    let next = planner::observe(
        &mut document,
        &[authority("a", 0.0, provisional + WEEK)],
        provisional,
    );
    assert_eq!(next.len(), 1);
}

#[test]
fn provider_fixed_reset_replaces_provisional_success() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    let launch = planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW).remove(0);
    planner::finish(
        &mut document,
        &launch.id,
        true,
        FailureReason::Unsuccessful,
        NOW + 5,
    );
    let fixed = NOW + WEEK - 25_000;
    assert!(
        planner::observe(&mut document, &[authority("a", 1.0, fixed)], NOW + 30_000).is_empty()
    );
    assert_eq!(document.accounts[&account].active_until_ms, Some(fixed));
}

#[test]
fn automatic_failure_waits_for_fresh_usage_and_stops_after_three_retries() {
    let mut document = Document::new();
    let mut launch =
        planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW).remove(0);
    for (attempt, delay) in [60_000, 300_000, 900_000].into_iter().enumerate() {
        let failed_at = NOW + i64::try_from(attempt).unwrap() * 2_000_000;
        planner::finish(
            &mut document,
            &launch.id,
            false,
            FailureReason::SpawnFailed,
            failed_at,
        );
        assert!(planner::observe(
            &mut document,
            &[authority_fetched("a", 0.0, NOW + WEEK, failed_at)],
            failed_at + delay
        )
        .is_empty());
        launch = planner::observe(
            &mut document,
            &[authority_fetched("a", 0.0, NOW + WEEK, failed_at + 1)],
            failed_at + delay,
        )
        .remove(0);
    }
    planner::finish(
        &mut document,
        &launch.id,
        false,
        FailureReason::Unsuccessful,
        NOW + 9_000_000,
    );
    assert!(matches!(
        document.accounts[&AccountId::new("a")]
            .automatic
            .as_ref()
            .unwrap()
            .phase,
        JobPhase::NeedsAttention { .. }
    ));
}

#[test]
fn uncertain_automatic_outcome_is_never_sent_twice() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    let launch = planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW).remove(0);
    assert!(planner::finish(
        &mut document,
        &launch.id,
        false,
        FailureReason::Unsuccessful,
        NOW + 1,
    ));

    let launches = planner::observe(
        &mut document,
        &[authority_fetched("a", 0.0, NOW + WEEK, NOW + 2)],
        NOW + 2,
    );

    assert!(launches.is_empty());
    assert!(matches!(
        document.accounts[&account]
            .automatic
            .as_ref()
            .unwrap()
            .phase,
        JobPhase::NeedsAttention { .. }
    ));
}

#[test]
fn dead_process_owner_requires_attention_without_automatic_relaunch() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW);
    document
        .accounts
        .get_mut(&account)
        .unwrap()
        .automatic
        .as_mut()
        .unwrap()
        .owner = Some(ProcessOwner::missing_for_test());

    let launches = planner::observe(
        &mut document,
        &[authority_fetched("a", 0.0, NOW + WEEK, NOW + 1)],
        NOW + 1,
    );

    assert!(launches.is_empty());
    assert!(matches!(
        document.accounts[&account]
            .automatic
            .as_ref()
            .unwrap()
            .phase,
        JobPhase::NeedsAttention { .. }
    ));
}

#[test]
fn manual_and_automatic_tasks_never_claim_the_same_account_together() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    let launch = planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW).remove(0);
    assert_eq!(
        requests::manual(
            &mut document,
            &account,
            ProcessOwner::current().unwrap(),
            NOW
        ),
        ManualRequest::AlreadyRunning
    );
    planner::finish(
        &mut document,
        &launch.id,
        false,
        FailureReason::SpawnFailed,
        NOW + 1,
    );
    assert_eq!(
        requests::manual(
            &mut document,
            &account,
            ProcessOwner::current().unwrap(),
            NOW
        ),
        ManualRequest::AlreadyRunning
    );
}

#[test]
fn manual_double_click_creates_one_job() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    assert_eq!(
        requests::manual(
            &mut document,
            &account,
            ProcessOwner::current().unwrap(),
            NOW
        ),
        ManualRequest::Queued
    );
    let first = document.accounts[&account]
        .manual
        .as_ref()
        .unwrap()
        .id
        .clone();
    assert_eq!(
        requests::manual(
            &mut document,
            &account,
            ProcessOwner::current().unwrap(),
            NOW + 1,
        ),
        ManualRequest::AlreadyRunning
    );
    assert_eq!(
        document.accounts[&account].manual.as_ref().unwrap().id,
        first
    );
}

#[test]
fn dead_manual_owner_can_be_retried_without_the_router() {
    let account = AccountId::new("a");
    let owner = ProcessOwner::current().unwrap();
    let mut document = Document::new();
    assert_eq!(
        requests::manual(&mut document, &account, owner, NOW),
        ManualRequest::Queued
    );
    let original = document.accounts[&account]
        .manual
        .as_ref()
        .unwrap()
        .id
        .clone();
    document
        .accounts
        .get_mut(&account)
        .unwrap()
        .manual
        .as_mut()
        .unwrap()
        .owner = Some(ProcessOwner::missing_for_test());
    requests::reconcile_account(&mut document, &account, NOW + 1);
    assert_eq!(
        requests::status(&document, &account).manual,
        ManualTestStatus::Failed
    );

    assert_eq!(
        requests::manual(&mut document, &account, owner, NOW + 2),
        ManualRequest::Queued
    );
    assert_ne!(
        document.accounts[&account].manual.as_ref().unwrap().id,
        original
    );
}

#[test]
fn persisted_success_does_not_repeat_with_a_rolling_unused_reset() {
    let directory = tempfile::tempdir().unwrap();
    let store = super::store::Store::at(directory.path().join("activation.json"));
    let mut document = Document::new();
    let launch = planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW).remove(0);
    planner::finish(
        &mut document,
        &launch.id,
        true,
        FailureReason::Unsuccessful,
        NOW + 5,
    );
    store
        .update(|stored| {
            *stored = document.clone();
            StoreUpdate::Changed(())
        })
        .unwrap();
    let mut reloaded = store.load().unwrap();
    assert!(planner::observe(
        &mut reloaded,
        &[authority("a", 0.0, NOW + WEEK + 120_000)],
        NOW + 120_000,
    )
    .is_empty());
}

#[test]
fn opting_out_is_quiet_and_manual_status_is_independent() {
    let account = AccountId::new("a");
    let mut document = Document::new();
    requests::set_automatic(&mut document, &account, false);
    assert!(planner::observe(&mut document, &[authority("a", 0.0, NOW + WEEK)], NOW).is_empty());
    assert_eq!(
        requests::manual(
            &mut document,
            &account,
            ProcessOwner::current().unwrap(),
            NOW
        ),
        ManualRequest::Queued
    );
    let status = requests::status(&document, &account);
    assert!(!status.automatic_enabled);
    assert_eq!(status.automatic, AutomaticTestStatus::Ready);
    assert_eq!(status.manual, ManualTestStatus::Pending);
}

#[test]
fn authority_requires_live_current_exact_primary_proof() {
    let directory = tempfile::tempdir().unwrap();
    let mut profile = profile(directory.path());
    write_auth(directory.path(), "provider-account", "access-a");
    let auth = CodexAuthSnapshot::read(&profile).unwrap();
    profile.account.id = auth.account_id.clone();
    profile.account.identity_kind = AccountIdentityKind::ProviderPrincipal;
    profile.account.sources = vec![source(&profile.profile_id)];
    let proof = CodexAuthSnapshot::read(&profile).unwrap().proof();
    let snapshot = snapshot(profile.account.clone(), SnapshotFreshness::Live);
    let collection = ProviderLimitCollection {
        snapshots: vec![snapshot.clone()],
        codex_auth: vec![proof.clone()],
    };
    assert_eq!(
        super::authority::proved_for_test(&collection, &[profile.clone()], NOW).len(),
        1
    );
    let mut ambiguous_snapshot = snapshot.clone();
    ambiguous_snapshot
        .windows
        .push(ambiguous_snapshot.windows[0].clone());
    let ambiguous = ProviderLimitCollection {
        snapshots: vec![ambiguous_snapshot],
        codex_auth: vec![proof.clone()],
    };
    let ambiguous = super::authority::proved_for_test(&ambiguous, &[profile.clone()], NOW);
    assert!(ambiguous[0].weekly.is_none());
    assert!(planner::observe(&mut Document::new(), &ambiguous, NOW).is_empty());
    let mut scoped_snapshot = snapshot.clone();
    scoped_snapshot.windows[0].scope = Some("model".into());
    let scoped = ProviderLimitCollection {
        snapshots: vec![scoped_snapshot],
        codex_auth: vec![proof.clone()],
    };
    let scoped = super::authority::proved_for_test(&scoped, &[profile.clone()], NOW);
    assert!(scoped[0].weekly.is_none());
    assert!(planner::observe(&mut Document::new(), &scoped, NOW).is_empty());
    let cached = ProviderLimitCollection {
        snapshots: vec![snapshot_with_freshness(snapshot, SnapshotFreshness::Cached)],
        codex_auth: vec![proof.clone()],
    };
    assert!(super::authority::proved_for_test(&cached, &[profile.clone()], NOW).is_empty());
    write_auth(directory.path(), "provider-account", "access-b");
    assert!(super::authority::proved_for_test(&collection, &[profile], NOW).is_empty());
}

fn authority(id: &str, percent_used: f64, resets_at_ms: i64) -> Authority {
    authority_fetched(id, percent_used, resets_at_ms, NOW)
}

fn authority_fetched(
    id: &str,
    percent_used: f64,
    resets_at_ms: i64,
    fetched_at_ms: i64,
) -> Authority {
    Authority {
        account: AccountId::new(id),
        profile_id: CredentialProfileId::new(format!("profile-{id}")),
        fetched_at_ms,
        weekly: Some(WeeklyUsage {
            percent_used,
            resets_at_ms,
        }),
    }
}

fn profile(path: &std::path::Path) -> AccountProfile {
    AccountProfile {
        provider: Provider::Codex,
        profile_id: CredentialProfileId::new("profile"),
        account: ProviderAccount {
            id: AccountId::new("placeholder"),
            identity_kind: AccountIdentityKind::ProfileFallback,
            email: None,
            sources: Vec::new(),
        },
        home_dir: path.into(),
        config_dir: path.into(),
        managed: true,
        created_at_ms: Some(1),
    }
}

fn source(profile_id: &CredentialProfileId) -> AccountSource {
    AccountSource {
        profile_id: profile_id.clone(),
        kind: CredentialProfileKind::Managed,
        primary: true,
    }
}

fn snapshot(account: ProviderAccount, freshness: SnapshotFreshness) -> LimitSnapshot {
    LimitSnapshot {
        provider: Provider::Codex,
        account,
        plan: None,
        plan_multiplier: None,
        banked_resets: 0,
        banked_reset_credits: None,
        windows: vec![LimitWindow {
            id: "primary_window".into(),
            label: "Weekly".into(),
            percent_used: 0.0,
            resets_at: Utc.timestamp_millis_opt(NOW + WEEK).single(),
            severity: None,
            scope: None,
            is_active: true,
            raw: serde_json::Value::Null,
        }],
        extras: Vec::new(),
        fetched_at: Utc.timestamp_millis_opt(NOW).single(),
        source: "test".into(),
        issue: None,
        status: SnapshotStatus::at(freshness),
    }
}

fn snapshot_with_freshness(
    mut snapshot: LimitSnapshot,
    freshness: SnapshotFreshness,
) -> LimitSnapshot {
    snapshot.status = SnapshotStatus::at(freshness);
    snapshot
}

fn write_auth(path: &std::path::Path, account: &str, access: &str) {
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"RS256"}"#);
    let claims = URL_SAFE_NO_PAD.encode(serde_json::json!({"iss":"https://auth.openai.com","https://api.openai.com/auth":{"chatgpt_account_id":account}}).to_string());
    let signature = URL_SAFE_NO_PAD.encode([7_u8; 256]);
    let auth = serde_json::json!({"tokens":{"id_token":format!("{header}.{claims}.{signature}"),"access_token":access,"refresh_token":"refresh","account_id":account}});
    std::fs::write(path.join("auth.json"), serde_json::to_vec(&auth).unwrap()).unwrap();
}
