use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::storage::{LockMode, PrivateFileLock};

pub(super) struct RefreshLock {
    _file: PrivateFileLock,
}

impl RefreshLock {
    pub(super) async fn acquire(auth_path: &Path) -> Result<Self> {
        let path = lock_path(auth_path);
        tokio::task::spawn_blocking(move || Self::acquire_blocking(&path))
            .await
            .context("joining Codex credential refresh lock task")?
    }

    fn acquire_blocking(path: &Path) -> Result<Self> {
        let file =
            crate::storage::lock_private(path, "Codex credential refresh", LockMode::Blocking)
                .context("locking Codex credential refresh")?;
        Ok(Self { _file: file })
    }
}

fn lock_path(auth_path: &Path) -> PathBuf {
    let canonical = fs::canonicalize(auth_path).unwrap_or_else(|_| auth_path.to_path_buf());
    canonical.with_file_name(".toks-codex-refresh.lock")
}
