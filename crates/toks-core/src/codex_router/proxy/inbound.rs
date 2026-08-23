use std::collections::{BTreeMap, BTreeSet};
use std::sync::Mutex;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

use crate::accounts::AccountId;

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
        let path = toks_ingest::paths::get_data_dir()
            .map(|root| root.join("rotation/inbound-tokens.json"));
        Self::with_store(credentials, AdmissionStore::new(path))
    }

    #[cfg(test)]
    pub fn at(credentials: SharedCredentials, path: std::path::PathBuf) -> Self {
        Self::with_store(credentials, AdmissionStore::new(Some(path)))
    }

    fn with_store(credentials: SharedCredentials, store: AdmissionStore) -> Self {
        let active = credentials.account_ids().into_iter().collect();
        let mut admissions = store.load().unwrap_or_default();
        prune(&mut admissions, &active, now());
        let _ = store.save(&admissions);
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
        let now = now();
        let active = self.credentials.account_ids().into_iter().collect();
        let changed = prune(&mut validated, &active, now);
        if changed {
            let _ = self.store.save(&validated);
        }
        if validated.contains_key(&digest) {
            return true;
        }
        let Some(account_id) = self.credentials.incoming_account(token) else {
            return false;
        };
        if !active.contains(&account_id) {
            return false;
        }
        let expires_at = token_expiry(token);
        if expires_at.is_some_and(|expiry| expiry <= now) {
            return false;
        }
        validated.insert(
            digest,
            Admission {
                digest,
                account_id,
                expires_at,
            },
        );
        prune(&mut validated, &active, now);
        let _ = self.store.save(&validated);
        true
    }
}

fn prune(
    admissions: &mut BTreeMap<[u8; 32], Admission>,
    active: &BTreeSet<AccountId>,
    now: i64,
) -> bool {
    let before = admissions.len();
    admissions.retain(|_, admission| {
        active.contains(&admission.account_id)
            && admission
                .expires_at
                .is_none_or(|expires_at| expires_at > now)
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

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}
