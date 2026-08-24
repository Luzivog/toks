use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(crate) fn write_private_atomic(path: &Path, bytes: &[u8], label: &str) -> Result<()> {
    write_via_temporary(path, label, |file| {
        file.write_all(bytes).map_err(Into::into)
    })
}

pub(crate) fn write_private_atomic_capped(
    path: &Path,
    limit: u64,
    label: &str,
    write: impl FnOnce(&mut dyn Write) -> Result<()>,
) -> Result<()> {
    write_via_temporary(path, label, |file| {
        let mut writer = BoundedWriter {
            inner: file,
            remaining: limit,
        };
        write(&mut writer)?;
        writer.flush().map_err(Into::into)
    })
}

pub(crate) fn unique_temp_path(path: &Path) -> Result<PathBuf> {
    let parent = path.parent().context("temporary file path has no parent")?;
    let file = path
        .file_name()
        .and_then(|name| name.to_str())
        .context("temporary file path has no UTF-8 file name")?;
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before Unix epoch")?
        .as_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let pid = std::process::id();
    Ok(parent.join(format!(".{file}.{pid}-{nonce}-{sequence}.tmp")))
}

pub(crate) fn restrict_directory(path: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn write_via_temporary(
    destination: &Path,
    label: &str,
    write: impl FnOnce(&mut File) -> Result<()>,
) -> Result<()> {
    let parent = destination
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    let temporary = unique_temp_path(destination)?;
    let result = publish(&temporary, destination, label, write);
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

fn publish(
    temporary: &Path,
    destination: &Path,
    label: &str,
    write: impl FnOnce(&mut File) -> Result<()>,
) -> Result<()> {
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
    write(&mut file).with_context(|| format!("writing {label}"))?;
    file.sync_all()
        .with_context(|| format!("syncing {label}"))?;
    fs::rename(temporary, destination).with_context(|| format!("publishing {label}"))?;
    #[cfg(unix)]
    File::open(
        destination
            .parent()
            .context("published file has no parent")?,
    )?
    .sync_all()?;
    Ok(())
}

struct BoundedWriter<W> {
    inner: W,
    remaining: u64,
}

impl<W: Write> Write for BoundedWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("history snapshot exceeds size limit"));
        }
        let written = self.inner.write(bytes)?;
        self.remaining -= written as u64;
        Ok(written)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}
