use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use super::model::{Document, DOCUMENT_VERSION};
use crate::storage::{LockMode, PrivateFileLock};

#[derive(Clone, Debug)]
pub(super) struct Store {
    path: PathBuf,
}

impl Store {
    pub(super) fn discover() -> Result<Self> {
        Ok(Self::at(crate::paths::account_activation_store()?))
    }

    pub(super) fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub(super) fn load(&self) -> Result<Document> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Document::new())
            }
            Err(error) => return Err(error).context("reading account activation state"),
        };
        let document: Document =
            serde_json::from_slice(&bytes).context("parsing account activation state")?;
        anyhow::ensure!(
            document.version == DOCUMENT_VERSION,
            "unsupported account activation state version {}",
            document.version
        );
        Ok(document)
    }

    pub(super) fn update<T>(&self, change: impl FnOnce(&mut Document) -> (T, bool)) -> Result<T> {
        let _lock = lock(&self.path)?;
        let mut document = self.load()?;
        let (value, changed) = change(&mut document);
        if changed {
            let bytes = serde_json::to_vec_pretty(&document)?;
            crate::storage::write_private_atomic(&self.path, &bytes, "account activation state")?;
        }
        Ok(value)
    }
}

fn lock(path: &Path) -> Result<PrivateFileLock> {
    let parent = path
        .parent()
        .context("account activation path has no parent")?;
    fs::create_dir_all(parent).context("creating account activation directory")?;
    crate::storage::restrict_directory(parent)?;
    let mut name = path
        .file_name()
        .context("account activation path has no file name")?
        .to_os_string();
    name.push(".lock");
    crate::storage::lock_private(
        &parent.join(name),
        "account activation state",
        LockMode::Blocking,
    )
}
