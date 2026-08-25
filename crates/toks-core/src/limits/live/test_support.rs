use std::time::Duration;

use super::RefreshOutcome;
use crate::accounts::AccountProfile;

pub(crate) fn memoized_or_fetch(
    profile: &AccountProfile,
    fetch: impl FnOnce() -> RefreshOutcome,
) -> RefreshOutcome {
    let key = profile.cache_key();
    let credential_revision = super::super::credentials::revision(profile);
    let account_lock = super::memo::account_lock(&key);
    let _guard = account_lock
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    if let Some(outcome) = super::memo::get(&key, None, credential_revision) {
        return outcome;
    }
    let outcome = fetch();
    super::memo::remember(
        key,
        outcome.clone(),
        0,
        Duration::from_secs(60),
        credential_revision,
    );
    outcome
}
