use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::storage::{LockMode, PrivateFileLock};

pub(super) fn lock_document(path: &Path, label: &str) -> Result<PrivateFileLock> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    crate::storage::restrict_directory(parent)?;
    let mut name = path
        .file_name()
        .with_context(|| format!("{label} path has no file name"))?
        .to_os_string();
    name.push(".lock");
    crate::storage::lock_private(&parent.join(name), label, LockMode::Blocking)
}
