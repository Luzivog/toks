use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use super::LaunchContract;
use crate::codex_router::systemd::units::UnitEnvironment;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub(in crate::codex_router::systemd) fn capture(
    root: &Path,
    executable: &Path,
    codex_executable: &Path,
    inherited: &UnitEnvironment,
) -> Result<LaunchContract> {
    LaunchContract::capture(&materialize(root, executable)?, codex_executable, inherited)
}

pub(super) fn materialize(root: &Path, source: &Path) -> Result<PathBuf> {
    let bytes = fs::read(source).context("reading router candidate")?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    let destination = root.join("executables").join(hash).join("toks-router");
    let parent = destination
        .parent()
        .context("router artifact has no parent")?;
    super::path_guard::prepare(root, parent)?;
    if destination.exists() || destination.is_symlink() {
        validate(&destination, &bytes)?;
        return destination.canonicalize().map_err(Into::into);
    }

    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".toks-router.{}-{sequence}.tmp",
        std::process::id()
    ));
    crate::rotation::write_private_atomic(&temporary, &bytes, "router executable artifact")?;
    fs::set_permissions(&temporary, fs::Permissions::from_mode(0o755))?;
    let published = match fs::hard_link(&temporary, &destination) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            validate(&destination, &bytes)
        }
        Err(error) => Err(error).context("publishing router executable artifact"),
    };
    let cleanup = fs::remove_file(&temporary);
    published?;
    cleanup?;
    fs::File::open(parent)?.sync_all()?;
    destination.canonicalize().map_err(Into::into)
}

fn validate(path: &Path, expected: &[u8]) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    anyhow::ensure!(
        metadata.file_type().is_file(),
        "router artifact is not a regular file"
    );
    anyhow::ensure!(
        fs::read(path)? == expected,
        "router artifact hash collision"
    );
    anyhow::ensure!(
        metadata.permissions().mode() & 0o111 == 0o111,
        "router artifact is not executable"
    );
    Ok(())
}
