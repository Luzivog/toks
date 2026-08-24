use super::removal::confirmation_body;
use toks_core::accounts::AccountOrigin;

#[test]
fn removal_copy_matches_capability() {
    assert!(confirmation_body(AccountOrigin::Managed, "Codex")
        .contains("saved credentials will be removed"));
    assert!(confirmation_body(AccountOrigin::Current, "Codex")
        .contains("credentials will not be changed"));
    assert!(
        confirmation_body(AccountOrigin::Mixed, "Claude Code").contains("profiles will be removed")
    );
}
