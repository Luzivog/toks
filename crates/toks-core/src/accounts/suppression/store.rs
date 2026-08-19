use anyhow::{bail, Context, Result};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::limits::{LimitSnapshot, Provider};

use super::super::{AccountId, CredentialProfileKind, ProviderAccount};
use super::filtering::{retain_visible, update_for_observed_accounts};
use super::model::{SuppressedAccount, SuppressionDocument, DOCUMENT_VERSION};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
static FILE_LOCK: OnceLock<Mutex<()>> = OnceLock::new();

pub(super) struct SuppressionStore {
    path: PathBuf,
}

impl SuppressionStore {
    pub(super) fn default() -> Result<Self> {
        let root = toks_ingest::paths::get_data_dir().context("no local data directory")?;
        Ok(Self::at(root.join("account-suppression.json")))
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
        super::super::restrict_directory(parent)?;
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .context("system clock is before Unix epoch")?
            .as_nanos();
        let temporary = parent.join(format!(
            ".account-suppression-{}-{nonce}-{sequence}.tmp",
            std::process::id(),
        ));
        let bytes = serde_json::to_vec_pretty(document)?;
        let result = write_atomic(&temporary, &self.path, &bytes);
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result
    }
}

fn file_lock() -> std::sync::MutexGuard<'static, ()> {
    FILE_LOCK
        .get_or_init(|| Mutex::new(()))
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
}

fn write_atomic(temporary: &Path, destination: &Path, bytes: &[u8]) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(temporary)
        .context("creating suppression state")?;
    file.write_all(bytes).context("writing suppression state")?;
    file.sync_all().context("syncing suppression state")?;
    fs::rename(temporary, destination).context("publishing suppression state")?;
    fs::File::open(
        destination
            .parent()
            .context("suppression path has no parent")?,
    )
    .and_then(|directory| directory.sync_all())
    .context("syncing Toks data directory")
}
