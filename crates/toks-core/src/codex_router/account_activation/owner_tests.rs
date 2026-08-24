use super::owner::ProcessOwner;

#[test]
fn owner_identity_rejects_missing_or_reused_processes() {
    assert!(ProcessOwner::current().unwrap().is_alive());
    assert!(!ProcessOwner::missing_for_test().is_alive());
}
