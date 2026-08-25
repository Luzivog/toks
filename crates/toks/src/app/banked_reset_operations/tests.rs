use toks_core::{
    accounts::{AccountId, BankedResetResult, ProviderAccount},
    limits::BankedResetOutcome,
    rotation::UnixMillis,
    LimitSnapshot, Provider,
};

use super::{BankedResetOperations, BankedResetStatus, State};

#[test]
fn cancel_never_creates_a_redemption_attempt() {
    let account = AccountId::new("account");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    assert_eq!(operations.status(&account), BankedResetStatus::Confirming);

    operations.cancel(&account);

    assert_eq!(operations.status(&account), BankedResetStatus::Ready);
}

#[test]
fn retry_reuses_the_same_idempotent_attempt() {
    let account = AccountId::new("account");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    let first = operations.begin(&account, 2).unwrap();
    operations.finish(&account, &first, &Err(anyhow::anyhow!("synthetic timeout")));
    assert!(matches!(
        operations.status(&account),
        BankedResetStatus::Retry(_)
    ));

    let retry = operations.begin(&account, 2).unwrap();

    assert_eq!(retry, first);
    operations.finish(
        &account,
        &retry,
        &Ok(BankedResetResult {
            outcome: BankedResetOutcome::AlreadyRedeemed,
            routing_updated: true,
            redeemed_at: Some(toks_core::rotation::UnixMillis::new(1)),
        }),
    );
    assert_eq!(operations.status(&account), BankedResetStatus::Ready);
    assert_eq!(operations.redeemed_at(&account), Some(UnixMillis::new(1)));
    assert_eq!(operations.error(), None);
}

#[test]
fn retry_keeps_the_original_credit_count_after_a_refresh() {
    let account = AccountId::new("account");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    let attempt = operations.begin(&account, 2).unwrap();
    operations.finish(
        &account,
        &attempt,
        &Err(anyhow::anyhow!("synthetic timeout")),
    );

    operations.begin(&account, 0).unwrap();

    assert!(matches!(
        operations.state,
        State::Pending {
            starting_count: 2,
            ..
        }
    ));
}

#[test]
fn successful_reset_records_inline_refresh_evidence_without_a_notice() {
    let account = AccountId::new("account");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    let attempt = operations.begin(&account, 2).unwrap();

    operations.finish(
        &account,
        &attempt,
        &Ok(BankedResetResult {
            outcome: BankedResetOutcome::Reset,
            routing_updated: true,
            redeemed_at: Some(UnixMillis::new(42)),
        }),
    );

    assert_eq!(operations.status(&account), BankedResetStatus::Ready);
    assert_eq!(operations.redeemed_at(&account), Some(UnixMillis::new(42)));
    assert_eq!(operations.error(), None);
}

#[test]
fn routing_write_failure_keeps_success_inline_and_exposes_an_actionable_error() {
    let account = AccountId::new("account");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    let attempt = operations.begin(&account, 1).unwrap();

    operations.finish(
        &account,
        &attempt,
        &Ok(BankedResetResult {
            outcome: BankedResetOutcome::Reset,
            routing_updated: false,
            redeemed_at: Some(UnixMillis::new(42)),
        }),
    );

    assert_eq!(operations.redeemed_at(&account), Some(UnixMillis::new(42)));
    assert!(operations
        .error()
        .unwrap()
        .contains("Restart Codex routing"));
}

#[test]
fn cancelling_retry_releases_other_accounts_and_clears_its_error() {
    let account = AccountId::new("account");
    let other = AccountId::new("other");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    let attempt = operations.begin(&account, 1).unwrap();
    operations.finish(
        &account,
        &attempt,
        &Err(anyhow::anyhow!("synthetic timeout")),
    );
    assert_eq!(operations.status(&other), BankedResetStatus::Busy);
    assert!(operations.error().is_some());

    operations.cancel(&account);

    assert_eq!(operations.status(&account), BankedResetStatus::Ready);
    assert_eq!(operations.status(&other), BankedResetStatus::Ready);
    assert_eq!(operations.error(), None);
}

#[test]
fn refreshed_credit_count_acknowledges_an_ambiguous_redemption() {
    let account = AccountId::new("account");
    let mut operations = BankedResetOperations::default();
    operations.confirm(account.clone());
    let attempt = operations.begin(&account, 2).unwrap();
    operations.finish(
        &account,
        &attempt,
        &Err(anyhow::anyhow!("synthetic timeout")),
    );
    let mut refreshed = LimitSnapshot::loading_account(
        Provider::Codex,
        ProviderAccount {
            id: account.clone(),
            ..ProviderAccount::unidentified_for(Provider::Codex)
        },
    );
    refreshed.banked_resets = 1;

    operations.reconcile_with(&[refreshed], |_| Ok(UnixMillis::new(42)));

    assert_eq!(operations.status(&account), BankedResetStatus::Ready);
    assert_eq!(operations.redeemed_at(&account), Some(UnixMillis::new(42)));
    assert_eq!(operations.error(), None);
}
