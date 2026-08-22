use std::collections::BTreeSet;
use std::sync::Mutex;

use sha2::{Digest, Sha256};

use super::types::SharedCredentials;

pub(super) struct InboundTokens {
    validated: Mutex<BTreeSet<[u8; 32]>>,
    credentials: SharedCredentials,
}

impl InboundTokens {
    pub fn new(credentials: SharedCredentials) -> Self {
        Self {
            validated: Mutex::new(BTreeSet::new()),
            credentials,
        }
    }

    /// Remember only a digest so clients survive an account refresh without
    /// putting bearer tokens in router state or logs.
    pub fn accepts(&self, token: &str) -> bool {
        let digest = Sha256::digest(token.as_bytes()).into();
        let mut validated = self.validated.lock().expect("inbound token cache poisoned");
        if validated.contains(&digest) {
            return true;
        }
        if !self.credentials.accepts_incoming(token) {
            return false;
        }
        validated.insert(digest);
        true
    }
}
