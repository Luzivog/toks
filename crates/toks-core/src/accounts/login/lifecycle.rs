use std::sync::{Mutex, OnceLock};

use crate::limits::Provider;

use crate::accounts::CredentialProfileId;

mod registry;
mod watcher;
use registry::Registry;
pub(super) use watcher::{credential_stamp, track_add, track_reauthentication};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoginOutcome {
    /// The provider terminal or its credential write is still in progress.
    Pending,
    /// Credentials were written without replacing a known provider principal.
    Authenticated,
    /// Reauthentication replaced the profile's known provider principal.
    IdentityChanged,
    /// Toks cancelled tracking and terminated the attached terminal.
    Cancelled,
    /// No credential write was observed; the profile is retained for recovery.
    Abandoned,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct LoginKey {
    provider: Provider,
    profile_id: CredentialProfileId,
}

struct Tracking {
    generation: u64,
    cancelled: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

static REGISTRY: OnceLock<Mutex<Registry>> = OnceLock::new();

pub fn login_outcome(provider: Provider, profile_id: &CredentialProfileId) -> Option<LoginOutcome> {
    let key = LoginKey {
        provider,
        profile_id: profile_id.clone(),
    };
    with_registry(|registry| registry.outcome(&key))
}

/// Cancel lifecycle tracking for one exact credential profile and terminate
/// Toks's tracked sign-in terminal. Generation checks also make a late
/// completion harmless while removal quarantines the profile.
pub fn cancel_login(provider: Provider, profile_id: &CredentialProfileId) -> bool {
    let key = LoginKey {
        provider,
        profile_id: profile_id.clone(),
    };
    with_registry(|registry| registry.cancel(&key))
}

fn start(key: LoginKey) -> Tracking {
    with_registry(|registry| registry.start(key))
}

fn finish(key: &LoginKey, generation: u64, outcome: LoginOutcome) {
    with_registry(|registry| registry.finish(key, generation, outcome));
}

fn is_pending(key: &LoginKey, generation: u64) -> bool {
    with_registry(|registry| registry.is_pending(key, generation))
}

fn with_registry<T>(action: impl FnOnce(&mut Registry) -> T) -> T {
    let mut registry = REGISTRY
        .get_or_init(|| Mutex::new(Registry::default()))
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    action(&mut registry)
}

#[cfg(test)]
#[path = "lifecycle/tests.rs"]
mod tests;
