use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

pub(super) struct RefreshLock {
    _file: Flock<File>,
}

impl RefreshLock {
    pub(super) async fn acquire(auth_path: &Path) -> Result<Self> {
        let path = lock_path(auth_path);
        tokio::task::spawn_blocking(move || Self::acquire_blocking(&path))
            .await
            .context("joining Codex credential refresh lock task")?
    }

    fn acquire_blocking(path: &Path) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .mode(0o600)
            .custom_flags(nix::libc::O_CLOEXEC | nix::libc::O_NOFOLLOW)
            .open(path)
            .with_context(|| format!("opening Codex credential refresh lock {}", path.display()))?;
        let file = Flock::lock(file, FlockArg::LockExclusive)
            .map_err(|(_, error)| error)
            .context("locking Codex credential refresh")?;
        Ok(Self { _file: file })
    }
}

fn lock_path(auth_path: &Path) -> PathBuf {
    let canonical = fs::canonicalize(auth_path).unwrap_or_else(|_| auth_path.to_path_buf());
    canonical.with_file_name(".toks-codex-refresh.lock")
}
