use crate::accounts::{AccountId, CredentialProfileId};
use crate::limits::Provider;

use super::registry::Registry;
use super::watcher::classify_identity;
use super::{LoginKey, LoginOutcome};

fn key(id: &str) -> LoginKey {
    LoginKey {
        provider: Provider::Codex,
        profile_id: CredentialProfileId::new(id),
    }
}

#[test]
fn cancellation_wins_over_late_completion() {
    let mut registry = Registry::default();
    let key = key("cancelled");
    let tracking = registry.start(key.clone());
    assert!(registry.cancel(&key));
    assert!(tracking
        .cancelled
        .load(std::sync::atomic::Ordering::Acquire));
    registry.finish(&key, tracking.generation, LoginOutcome::Authenticated);
    assert_eq!(registry.outcome(&key), Some(LoginOutcome::Cancelled));
}

#[test]
fn stale_generation_cannot_finish_a_restarted_login() {
    let mut registry = Registry::default();
    let key = key("restarted");
    let stale = registry.start(key.clone());
    let current = registry.start(key.clone());
    assert!(stale.cancelled.load(std::sync::atomic::Ordering::Acquire));
    registry.finish(&key, stale.generation, LoginOutcome::IdentityChanged);
    assert_eq!(registry.outcome(&key), Some(LoginOutcome::Pending));
    registry.finish(&key, current.generation, LoginOutcome::Authenticated);
    assert_eq!(registry.outcome(&key), Some(LoginOutcome::Authenticated));
}

#[test]
fn changed_verified_principal_is_typed() {
    let before = AccountId::new("principal-a");
    let after = AccountId::new("principal-b");
    assert_eq!(
        classify_identity(Some(&before), &after),
        LoginOutcome::IdentityChanged
    );
    assert_eq!(
        classify_identity(Some(&after), &after),
        LoginOutcome::Authenticated
    );
    assert_eq!(classify_identity(None, &after), LoginOutcome::Authenticated);
}
