use super::activation::test_action;
use toks_core::codex_router::account_activation::{
    AccountActivationStatus, AutomaticTestStatus, ManualTestOutcome, ManualTestReceipt,
    ManualTestStatus,
};
use toks_core::{accounts::AccountId, rotation::ThreadId};

#[test]
fn account_test_action_is_state_specific() {
    let mut status = AccountActivationStatus::default();
    assert_eq!(test_action(&status), ("Send test", false));
    status.manual = ManualTestStatus::Running;
    assert_eq!(test_action(&status), ("Sending test…", true));
    status.manual = ManualTestStatus::Failed;
    assert_eq!(test_action(&status), ("Retry test", false));
    status.automatic = AutomaticTestStatus::Pending;
    assert_eq!(test_action(&status), ("Sending test…", true));
    status.automatic = AutomaticTestStatus::NeedsAttention;
    assert_eq!(test_action(&status), ("Retry weekly reset test", false));
}

#[test]
fn completed_test_keeps_a_durable_task_receipt_in_the_menu() {
    let status = AccountActivationStatus {
        manual: ManualTestStatus::Succeeded,
        manual_receipt: Some(ManualTestReceipt {
            requested_account: AccountId::new("selected"),
            observed_account: Some(AccountId::new("selected")),
            thread_id: Some(ThreadId::new("01a051c0-5ad2-7060-8039-bfd1373e0c95")),
            started_at_ms: 1_800_000_000_000,
            routed_at_ms: Some(1_800_000_001_000),
            completed_at_ms: 1_800_000_003_000,
            outcome: ManualTestOutcome::Succeeded,
        }),
        ..Default::default()
    };

    assert_eq!(
        super::activation::receipt_label(&status),
        Some("Last test succeeded on this account · task 01a051c0".to_owned())
    );
}
