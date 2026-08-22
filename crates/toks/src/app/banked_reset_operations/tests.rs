use toks_core::{
    accounts::{AccountId, BankedResetResult},
    limits::BankedResetOutcome,
};

use super::{BankedResetOperations, BankedResetStatus};

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
        }),
    );
    assert_eq!(operations.status(&account), BankedResetStatus::Ready);
}
