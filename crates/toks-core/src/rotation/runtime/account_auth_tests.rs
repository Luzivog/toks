use super::account_auth::{AccountAuthState, REJECTED_CREDENTIAL_HISTORY_LIMIT};

#[test]
fn rejected_credential_history_is_bounded_and_keeps_most_recent_unique_entries() {
    let mut auth = AccountAuthState::default();
    for index in 0..(REJECTED_CREDENTIAL_HISTORY_LIMIT + 2) {
        auth.remember_rejected_credential(&format!("fingerprint-{index}"));
    }

    auth.remember_rejected_credential("fingerprint-2");

    assert_eq!(
        auth.rejected_credential_history.len(),
        REJECTED_CREDENTIAL_HISTORY_LIMIT
    );
    assert!(!auth.credential_was_rejected("fingerprint-0"));
    assert!(!auth.credential_was_rejected("fingerprint-1"));
    assert!(auth.credential_was_rejected("fingerprint-2"));
    assert!(auth.credential_was_rejected(&format!(
        "fingerprint-{}",
        REJECTED_CREDENTIAL_HISTORY_LIMIT + 1
    )));
}
