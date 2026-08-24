use anyhow::{bail, Context, Result};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::limits::{LimitSnapshot, Provider};

use super::filtering::{retain_visible, update_for_observed_accounts};
use super::model::{SuppressedAccount, SuppressionDocument, DOCUMENT_VERSION};
use crate::accounts::{AccountId, CredentialProfileKind, ProviderAccount};

static FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) struct SuppressionStore {
    path: PathBuf,
}

impl SuppressionStore {
    pub(super) fn default() -> Result<Self> {
        Ok(Self::at(crate::paths::account_suppression_store()?))
    }

    pub(super) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub(super) fn hide(&self, provider: Provider, account: &ProviderAccount) -> Result<()> {
        let _guard = file_lock();
        let mut document = self.load()?;
        let current_profile_ids = account
            .sources
            .iter()
            .filter(|source| source.kind == CredentialProfileKind::Current)
            .map(|source| source.profile_id.clone())
            .collect();
        document
            .accounts
            .retain(|hidden| hidden.provider != provider || hidden.account_id != account.id);
        document.accounts.push(SuppressedAccount {
            provider,
            account_id: account.id.clone(),
            current_profile_ids,
        });
        document.normalize();
        self.save(&document)
    }

    pub(super) fn unhide(&self, provider: Provider, account_id: &AccountId) -> Result<bool> {
        let _guard = file_lock();
        let mut document = self.load()?;
        let before = document.accounts.len();
        document
            .accounts
            .retain(|hidden| hidden.provider != provider || &hidden.account_id != account_id);
        if document.accounts.len() == before {
            return Ok(false);
        }
        self.save(&document)?;
        Ok(true)
    }

    pub(super) fn filter(&self, snapshots: Vec<LimitSnapshot>) -> Vec<LimitSnapshot> {
        let _guard = file_lock();
        let Ok(mut document) = self.load() else {
            return snapshots;
        };
        let original = document.clone();
        if update_for_observed_accounts(&mut document, &snapshots) && self.save(&document).is_err()
        {
            return retain_visible(snapshots, &original);
        }
        retain_visible(snapshots, &document)
    }

    fn load(&self) -> Result<SuppressionDocument> {
        let raw = match fs::read(&self.path) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(SuppressionDocument::default())
            }
            Err(error) => return Err(error).context("reading account suppression state"),
        };
        let mut document: SuppressionDocument =
            serde_json::from_slice(&raw).context("parsing account suppression state")?;
        if document.version != DOCUMENT_VERSION {
            bail!(
                "unsupported account suppression version {}",
                document.version
            );
        }
        document.normalize();
        Ok(document)
    }

    fn save(&self, document: &SuppressionDocument) -> Result<()> {
        let parent = self
            .path
            .parent()
            .context("suppression path has no parent")?;
        fs::create_dir_all(parent).context("creating Toks data directory")?;
        crate::storage::restrict_directory(parent)?;
        let bytes = serde_json::to_vec_pretty(document)?;
        crate::storage::write_private_atomic(&self.path, &bytes, "suppression state")
    }
}

fn file_lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}
