use chrono::{Duration, TimeZone, Utc};
use toks_core::{
    accounts::{AccountId, ProviderAccount},
    rotation::{BlockWindow, RotationRuntime, UnixMillis},
    LimitSnapshot, LimitWindow, Provider,
};

use super::state::{derive_account_state, DerivedAccountState};

#[test]
fn stale_unavailability_becomes_reset_refreshing_until_new_evidence_arrives() {
    let account = AccountId::new("account");
    let redeemed_at = UnixMillis::new(2_000);
    let now = Utc.timestamp_millis_opt(3_000).single().unwrap();
    let mut runtime = blocked_runtime(&account);
    let mut limits = snapshot(&account, 50.0, 1_000);

    let stale = derive_account_state(
        &limits,
        runtime.accounts().get(&account),
        Some(redeemed_at),
        now,
    );
    assert_eq!(stale, DerivedAccountState::ResetRefreshing);
    assert_eq!(stale.label(), "Reset used · Refreshing limits…");

    limits.fetched_at = Utc.timestamp_millis_opt(2_001).single();
    assert_eq!(
        derive_account_state(
            &limits,
            runtime.accounts().get(&account),
            Some(redeemed_at),
            now,
        ),
        DerivedAccountState::Available
    );

    runtime.banked_reset_consumed(&account, redeemed_at);
    assert_eq!(
        derive_account_state(
            &snapshot(&account, 50.0, 1_000),
            runtime.accounts().get(&account),
            Some(redeemed_at),
            now,
        ),
        DerivedAccountState::Available
    );
}

#[test]
fn equal_timestamp_is_stale_and_post_ack_unavailability_is_truthful() {
    let account = AccountId::new("account");
    let redeemed_at = UnixMillis::new(2_000);
    let now = Utc.timestamp_millis_opt(3_000).single().unwrap();
    let equal = snapshot(&account, 50.0, redeemed_at.get());
    let mut runtime = blocked_runtime(&account);
    assert_eq!(
        derive_account_state(
            &equal,
            runtime.accounts().get(&account),
            Some(redeemed_at),
            now,
        ),
        DerivedAccountState::ResetRefreshing
    );

    runtime.banked_reset_consumed(&account, redeemed_at);
    runtime.block_admission(
        &account,
        BlockWindow::known(UnixMillis::new(10_000)),
        UnixMillis::new(2_001),
    );
    assert!(matches!(
        derive_account_state(
            &equal,
            runtime.accounts().get(&account),
            Some(redeemed_at),
            now,
        ),
        DerivedAccountState::Blocked { .. }
    ));
}

#[test]
fn fresh_post_reset_quota_can_truthfully_report_a_real_drain() {
    let account = AccountId::new("account");
    let redeemed_at = UnixMillis::new(2_000);
    let now = Utc.timestamp_millis_opt(3_000).single().unwrap();
    let runtime = blocked_runtime(&account);

    assert!(matches!(
        derive_account_state(
            &snapshot(&account, 99.0, 2_001),
            runtime.accounts().get(&account),
            Some(redeemed_at),
            now,
        ),
        DerivedAccountState::Draining { .. }
    ));
}

#[test]
fn reset_refreshing_never_masks_a_sign_in_failure() {
    let account = AccountId::new("account");
    let redeemed_at = UnixMillis::new(2_000);
    let now = Utc.timestamp_millis_opt(3_000).single().unwrap();
    let mut runtime = blocked_runtime(&account);
    runtime.auth_failed(&account, UnixMillis::new(1_500));

    assert_eq!(
        derive_account_state(
            &snapshot(&account, 50.0, 1_000),
            runtime.accounts().get(&account),
            Some(redeemed_at),
            now,
        ),
        DerivedAccountState::NeedsSignIn
    );
}

fn blocked_runtime(account: &AccountId) -> RotationRuntime {
    let mut runtime = RotationRuntime::default();
    runtime.reconcile(std::slice::from_ref(account), UnixMillis::new(0));
    runtime.block_admission(
        account,
        BlockWindow::known(UnixMillis::new(10_000)),
        UnixMillis::new(1_000),
    );
    runtime
}

fn snapshot(account: &AccountId, percent_used: f64, fetched_at_ms: i64) -> LimitSnapshot {
    let mut snapshot = LimitSnapshot::loading_account(
        Provider::Codex,
        ProviderAccount {
            id: account.clone(),
            ..ProviderAccount::unidentified_for(Provider::Codex)
        },
    );
    snapshot.fetched_at = Utc.timestamp_millis_opt(fetched_at_ms).single();
    snapshot.windows.push(LimitWindow {
        id: "weekly".into(),
        label: "Weekly".into(),
        percent_used,
        resets_at: Some(Utc.timestamp_millis_opt(10_000).single().unwrap() + Duration::days(7)),
        severity: None,
        scope: None,
        is_active: true,
        raw: serde_json::json!({}),
    });
    snapshot
}
