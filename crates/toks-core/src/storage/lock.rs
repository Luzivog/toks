use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

use anyhow::{Context, Result};
use nix::fcntl::{Flock, FlockArg};

pub(crate) struct PrivateFileLock {
    _file: Flock<File>,
}

#[derive(Clone, Copy)]
pub(crate) enum LockMode {
    Blocking,
    Nonblocking,
}

pub(crate) fn lock_private(path: &Path, label: &str, mode: LockMode) -> Result<PrivateFileLock> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(nix::libc::O_CLOEXEC | nix::libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("opening {label} lock {}", path.display()))?;
    file.set_permissions(fs::Permissions::from_mode(0o600))
        .with_context(|| format!("restricting {label} lock"))?;
    let argument = match mode {
        LockMode::Blocking => FlockArg::LockExclusive,
        LockMode::Nonblocking => FlockArg::LockExclusiveNonblock,
    };
    let file = Flock::lock(file, argument)
        .map_err(|(_, error)| error)
        .with_context(|| format!("locking {label}"))?;
    Ok(PrivateFileLock { _file: file })
}
