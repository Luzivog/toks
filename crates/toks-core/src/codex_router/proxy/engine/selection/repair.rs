use std::collections::BTreeMap;

use anyhow::Result;

use crate::accounts::AccountId;

use super::super::super::types::RouteCredential;
use super::super::Engine;

impl Engine {
    pub(super) async fn repaired_credentials(
        &self,
    ) -> Result<BTreeMap<AccountId, RouteCredential>> {
        let accounts = self.credentials.account_ids();
        let needs_proof = self.runtime.latest(|runtime| {
            accounts
                .into_iter()
                .filter_map(|account| {
                    runtime
                        .auth_failure(&account)
                        .map(|failure| (account, failure))
                })
                .collect::<Vec<_>>()
        })?;
        let mut repaired = BTreeMap::new();
        for (account, failure) in needs_proof {
            let Ok(credential) = self.credentials.credential(&account).await else {
                continue;
            };
            let fingerprint = credential.fingerprint();
            if credential.account_id != account
                || self.credential_was_rejected(&credential)?
                || failure
                    .2
                    .as_ref()
                    .is_some_and(|rejected| rejected == &fingerprint)
            {
                continue;
            }
            let restored = self.runtime.update(|runtime| {
                let restored =
                    runtime.sign_in_restored_by_proof(&account, failure.clone(), &fingerprint);
                (restored, restored)
            })?;
            if restored {
                repaired.insert(account, credential);
            }
        }
        Ok(repaired)
    }
}
