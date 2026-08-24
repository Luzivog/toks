use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

use super::named_unit_path;

const LOCK_NAME: &str = ".toks-router-lifecycle.lock";

pub(in crate::codex_router) struct LifecycleGuard {
    _file: Flock<File>,
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
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(nix::libc::O_CLOEXEC | nix::libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("opening router lifecycle lock {}", path.display()))?;
        file.set_permissions(fs::Permissions::from_mode(0o600))
            .context("restricting router lifecycle lock")?;
        let file = Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_, error)| error)
            .context("locking router install lifecycle")?;
        Ok(Self { _file: file })
    }
}
