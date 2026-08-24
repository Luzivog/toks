use std::fs;
use std::io::Read;
use std::path::Path;

use crate::storage::LockMode;

pub(super) const KEY_BYTES: usize = 32;
const LOCK_FILE: &str = ".account-principal.lock";

pub(super) fn load_or_create() -> Option<Vec<u8>> {
    let path = crate::paths::account_identity_key().ok()?;
    load_or_create_at(&path)
}

fn load_or_create_at(path: &Path) -> Option<Vec<u8>> {
    if let Some(key) = read(path) {
        return Some(key);
    }
    let parent = path.parent()?;
    fs::create_dir_all(parent).ok()?;
    crate::storage::restrict_directory(parent).ok()?;
    let _lock = crate::storage::lock_private(
        &parent.join(LOCK_FILE),
        "account principal",
        LockMode::Blocking,
    )
    .ok()?;
    if let Some(key) = read(path) {
        return Some(key);
    }
    let mut key = vec![0_u8; KEY_BYTES];
    getrandom::fill(&mut key).ok()?;
    crate::storage::write_private_atomic(path, &key, "account principal key").ok()?;
    Some(key)
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
