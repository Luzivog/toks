use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write_private_atomic(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    let temporary = temporary_path(path)?;
    let result = write_atomic(&temporary, path, bytes, label);
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn temporary_path(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().context("rotation state path has no parent")?;
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("rotation state path has no UTF-8 file name")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    Ok(parent.join(format!(".{file}.{pid}-{nonce}-{sequence}.tmp")))
}

fn write_atomic(temporary: &Path, destination: &Path, bytes: &[u8], label: &str) -> Result<()> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(temporary)
        .with_context(|| format!("creating {label}"))?;
    file.write_all(bytes)
        .with_context(|| format!("writing {label}"))?;
    file.sync_all()
        .with_context(|| format!("syncing {label}"))?;
    fs::rename(temporary, destination).with_context(|| format!("publishing {label}"))?;
    #[cfg(unix)]
    fs::File::open(
        destination
            .parent()
            .context("rotation state has no parent")?,
    )?
    .sync_all()?;
    Ok(())
}

pub(super) fn restrict_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}
