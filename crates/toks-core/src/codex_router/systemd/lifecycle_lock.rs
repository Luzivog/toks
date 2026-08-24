use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use super::named_unit_path;
use crate::storage::{LockMode, PrivateFileLock};

const LOCK_NAME: &str = ".toks-router-lifecycle.lock";

pub(in crate::codex_router) struct LifecycleGuard {
    _file: PrivateFileLock,
}

impl LifecycleGuard {
    pub(in crate::codex_router) fn acquire() -> Result<Self> {
        Self::acquire_path(&named_unit_path(LOCK_NAME)?)
    }

    #[cfg(test)]
    pub(in crate::codex_router) fn acquire_at(path: &Path) -> Result<Self> {
        Self::acquire_path(path)
    }

    fn acquire_path(path: &Path) -> Result<Self> {
        let parent = path
            .parent()
            .context("router lifecycle lock has no parent")?;
        fs::create_dir_all(parent).context("creating router systemd directory")?;
        let file = crate::storage::lock_private(path, "router lifecycle", LockMode::Blocking)
            .context("locking router install lifecycle")?;
        Ok(Self { _file: file })
    }
}
