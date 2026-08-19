use std::fs;
use std::path::{Path, PathBuf};
use std::process::Child;
use std::sync::atomic::Ordering;
use std::time::{Duration, UNIX_EPOCH};

use crate::limits::Provider;

use super::super::super::{AccountId, AccountProfile, CredentialProfileId};
use super::{finish, is_pending, start, LoginKey, LoginOutcome};

const LOGIN_SETTLE_ATTEMPTS: usize = 300;
const LOGIN_SETTLE_INTERVAL: Duration = Duration::from_secs(1);

pub(crate) fn track_add(
    child: Child,
    provider: Provider,
    profile_id: CredentialProfileId,
    config: PathBuf,
) {
    let key = LoginKey {
        provider,
        profile_id,
    };
    let tracking = start(key.clone());
    std::thread::spawn(move || {
        if !wait_for_terminal(child, &tracking) {
            return;
        }
        let outcome = wait_for_outcome(&key, tracking.generation, || {
            credentials_file(provider, &config)
                .is_file()
                .then_some(LoginOutcome::Authenticated)
        });
        finish(&key, tracking.generation, outcome);
    });
}

pub(crate) fn track_reauthentication(
    child: Child,
    profile: AccountProfile,
    before: Option<AccountId>,
    before_stamp: Option<CredentialStamp>,
) {
    let key = LoginKey {
        provider: profile.provider,
        profile_id: profile.profile_id.clone(),
    };
    let tracking = start(key.clone());
    std::thread::spawn(move || {
        if !wait_for_terminal(child, &tracking) {
            return;
        }
        let outcome = wait_for_outcome(&key, tracking.generation, || {
            let changed = credential_stamp(profile.provider, &profile.config_dir) != before_stamp;
            if !changed {
                return None;
            }
            let after = super::super::super::provider_principal_id(&profile)?;
            Some(classify_identity(before.as_ref(), &after))
        });
        finish(&key, tracking.generation, outcome);
    });
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CredentialStamp {
    modified_ns: u128,
    length: u64,
}

pub(crate) fn credential_stamp(provider: Provider, config: &Path) -> Option<CredentialStamp> {
    let metadata = fs::metadata(credentials_file(provider, config)).ok()?;
    Some(CredentialStamp {
        modified_ns: metadata
            .modified()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_nanos(),
        length: metadata.len(),
    })
}

pub(super) fn classify_identity(before: Option<&AccountId>, after: &AccountId) -> LoginOutcome {
    match before {
        Some(before) if before != after => LoginOutcome::IdentityChanged,
        _ => LoginOutcome::Authenticated,
    }
}

fn wait_for_outcome(
    key: &LoginKey,
    generation: u64,
    mut observe: impl FnMut() -> Option<LoginOutcome>,
) -> LoginOutcome {
    for _ in 0..LOGIN_SETTLE_ATTEMPTS {
        if !is_pending(key, generation) {
            return LoginOutcome::Cancelled;
        }
        if let Some(outcome) = observe() {
            return outcome;
        }
        std::thread::sleep(LOGIN_SETTLE_INTERVAL);
    }
    LoginOutcome::Abandoned
}

fn wait_for_terminal(mut child: Child, tracking: &super::Tracking) -> bool {
    loop {
        if tracking.cancelled.load(Ordering::Acquire) {
            let _ = child.kill();
            let _ = child.wait();
            return false;
        }
        match child.try_wait() {
            Ok(Some(_)) => return true,
            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
            Err(_) => return true,
        }
    }
}

fn credentials_file(provider: Provider, config: &Path) -> PathBuf {
    match provider {
        Provider::Claude => config.join(".credentials.json"),
        Provider::Codex => config.join("auth.json"),
    }
}
