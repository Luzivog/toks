use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

use crate::accounts::AccountId;
use crate::rotation::UnixMillis;
use crate::storage::StoreUpdate;

use super::types::SharedCredentials;

mod store;
use store::AdmissionStore;

const MAX_ADMISSIONS: usize = 64;

pub(super) struct InboundTokens {
    validated: Mutex<BTreeMap<[u8; 32], Admission>>,
    credentials: SharedCredentials,
    store: AdmissionStore,
}

#[derive(Clone, serde::Deserialize, serde::Serialize)]
pub(super) struct Admission {
    digest: [u8; 32],
    account_id: AccountId,
    expires_at: Option<i64>,
}

impl InboundTokens {
    pub fn new(credentials: SharedCredentials) -> Self {
        let path = crate::paths::proxy_inbound_store().ok();
        Self::with_store(credentials, AdmissionStore::new(path))
    }

    #[cfg(test)]
    pub fn at(credentials: SharedCredentials, path: std::path::PathBuf) -> Self {
        Self::with_store(credentials, AdmissionStore::new(Some(path)))
    }

    fn with_store(credentials: SharedCredentials, store: AdmissionStore) -> Self {
        let active = credentials.account_ids().into_iter().collect();
        let admissions = store
            .update(|admissions| {
                let changed = prune(admissions, &active, UnixMillis::now());
                StoreUpdate::from_changed(admissions.clone(), changed)
            })
            .unwrap_or_default();
        Self {
            validated: Mutex::new(admissions),
            credentials,
            store,
        }
    }

    /// Remember only a digest so clients survive an account refresh without
    /// putting bearer tokens in router state or logs.
    pub fn accepts(&self, token: &str) -> bool {
        let digest = Sha256::digest(token.as_bytes()).into();
        let mut validated = self.validated.lock().expect("inbound token cache poisoned");
        let now = UnixMillis::now();
        let active = self.credentials.account_ids().into_iter().collect();
        let changed = prune(&mut validated, &active, now);
        if validated.contains_key(&digest) {
            if changed {
                self.refresh_from_store(&mut validated, &active, now);
            }
            return true;
        }
        if !self.store.is_persistent() {
            return admission(&self.credentials, token, digest, &active, now).is_some_and(
                |admission| {
                    validated.insert(digest, admission);
                    prune(&mut validated, &active, now);
                    true
                },
            );
        }
        match self.store.update(|admissions| {
            let mut changed = prune(admissions, &active, now);
            let accepted = if let std::collections::btree_map::Entry::Vacant(entry) =
                admissions.entry(digest)
            {
                admission(&self.credentials, token, digest, &active, now).is_some_and(|admission| {
                    entry.insert(admission);
                    changed = true;
                    true
                })
            } else {
                true
            };
            changed |= prune(admissions, &active, now);
            StoreUpdate::from_changed((accepted, admissions.clone()), changed)
        }) {
            Ok((accepted, current)) => {
                merge_store_state(&mut validated, current);
                accepted
            }
            Err(_) => {
                admission(&self.credentials, token, digest, &active, now).is_some_and(|admission| {
                    validated.insert(digest, admission);
                    prune(&mut validated, &active, now);
                    true
                })
            }
        }
    }

    fn refresh_from_store(
        &self,
        validated: &mut BTreeMap<[u8; 32], Admission>,
        active: &BTreeSet<AccountId>,
        now: UnixMillis,
    ) {
        if let Ok(current) = self.store.update(|admissions| {
            let changed = prune(admissions, active, now);
            StoreUpdate::from_changed(admissions.clone(), changed)
        }) {
            merge_store_state(validated, current);
        }
    }
}

fn admission(
    credentials: &SharedCredentials,
    token: &str,
    digest: [u8; 32],
    active: &BTreeSet<AccountId>,
    now: UnixMillis,
) -> Option<Admission> {
    let account_id = credentials.incoming_account(token)?;
    if !active.contains(&account_id) {
        return None;
    }
    let expires_at = token_expiry(token);
    if expires_at.is_some_and(|expiry| expiry <= now.get().div_euclid(1_000)) {
        return None;
    }
    Some(Admission {
        digest,
        account_id,
        expires_at,
    })
}

fn merge_store_state(
    validated: &mut BTreeMap<[u8; 32], Admission>,
    mut current: BTreeMap<[u8; 32], Admission>,
) {
    for (digest, admission) in validated.iter() {
        if admission.expires_at.is_none() {
            current.insert(*digest, admission.clone());
        }
    }
    *validated = current;
}

fn prune(
    admissions: &mut BTreeMap<[u8; 32], Admission>,
    active: &BTreeSet<AccountId>,
    now: UnixMillis,
) -> bool {
    let now_seconds = now.get().div_euclid(1_000);
    let before = admissions.len();
    admissions.retain(|_, admission| {
        active.contains(&admission.account_id)
            && admission
                .expires_at
                .is_none_or(|expires_at| expires_at > now_seconds)
    });
    while admissions.len() > MAX_ADMISSIONS {
        let oldest = admissions
            .iter()
            .min_by_key(|(_, admission)| admission.expires_at.unwrap_or(i64::MIN))
            .map(|(digest, _)| *digest)
            .expect("non-empty admission cache");
        admissions.remove(&oldest);
    }
    admissions.len() != before
}

fn token_expiry(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = URL_SAFE_NO_PAD.decode(payload).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes)
        .ok()?
        .get("exp")?
        .as_i64()
}
