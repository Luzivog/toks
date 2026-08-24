use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use nix::fcntl::{Flock, FlockArg};

pub(super) const KEY_BYTES: usize = 32;
const KEY_FILE: &str = "account-principal.key";
const LOCK_FILE: &str = ".account-principal.lock";

pub(super) fn load_or_create() -> Option<Vec<u8>> {
    let path = toks_ingest::paths::get_data_dir()?.join(KEY_FILE);
    load_or_create_at(&path)
}

fn load_or_create_at(path: &Path) -> Option<Vec<u8>> {
    if let Some(key) = read(path) {
        return Some(key);
    }
    let parent = path.parent()?;
    fs::create_dir_all(parent).ok()?;
    super::super::super::restrict_directory(parent).ok()?;
    let lock = open_private(&parent.join(LOCK_FILE), false)?;
    let _lock = Flock::lock(lock, FlockArg::LockExclusive).ok()?;
    if let Some(key) = read(path) {
        return Some(key);
    }
    publish(path)
}

fn publish(path: &Path) -> Option<Vec<u8>> {
    let parent = path.parent()?;
    let temporary = temporary_path(path)?;
    let mut key = vec![0_u8; KEY_BYTES];
    getrandom::fill(&mut key).ok()?;
    let result = (|| {
        let mut file = open_private(&temporary, true)?;
        file.write_all(&key).ok()?;
        file.sync_all().ok()?;
        fs::rename(&temporary, path).ok()?;
        fs::File::open(parent).ok()?.sync_all().ok()?;
        Some(())
    })();
    if result.is_none() {
        let _ = fs::remove_file(&temporary);
        return None;
    }
    Some(key)
}

fn open_private(path: &Path, create_new: bool) -> Option<fs::File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    if create_new {
        options.create_new(true);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).ok()
}

fn temporary_path(path: &Path) -> Option<PathBuf> {
    let parent = path.parent()?;
    let name = path.file_name()?.to_string_lossy();
    Some(parent.join(format!(".{name}.{}.tmp", uuid::Uuid::new_v4())))
}

fn read(path: &Path) -> Option<Vec<u8>> {
    let metadata = fs::symlink_metadata(path).ok()?;
    if !metadata.file_type().is_file() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).ok()?;
    }
    let mut key = Vec::new();
    fs::File::open(path).ok()?.read_to_end(&mut key).ok()?;
    (key.len() == KEY_BYTES).then_some(key)
}

#[cfg(test)]
mod tests;
