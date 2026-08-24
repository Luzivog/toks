use std::fs::{self, File, OpenOptions};
use std::path::Path;

use anyhow::{Context, Result};

use super::atomic::restrict_directory;

pub(super) fn lock_document(path: &Path, label: &str) -> Result<File> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    restrict_directory(parent)?;
    let mut name = path
        .file_name()
        .with_context(|| format!("{label} path has no file name"))?
        .to_os_string();
    name.push(".lock");
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let lock = options.open(parent.join(name))?;
    lock.lock().with_context(|| format!("locking {label}"))?;
    Ok(lock)
}
