use super::activation::test_action;
use toks_core::codex_router::account_activation::{
    AccountActivationStatus, AutomaticTestStatus, ManualTestStatus,
};

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
