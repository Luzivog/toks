use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Component, Path, PathBuf};

use crate::Provider;

#[derive(Debug, Clone)]
pub(super) struct LifecyclePaths {
    pub(super) profiles_root: PathBuf,
    pub(super) order_path: PathBuf,
}

impl LifecyclePaths {
    pub(super) fn quarantine(&self, provider: Provider, id: &str) -> PathBuf {
        self.lifecycle_root()
            .join("quarantine")
            .join(provider.slug())
            .join(id)
    }

    pub(super) fn tombstone(&self, provider: Provider, id: &str) -> PathBuf {
        self.lifecycle_root()
            .join("removed")
            .join(provider.slug())
            .join(format!("{id}.json"))
    }

    fn lifecycle_root(&self) -> PathBuf {
        self.profiles_root.join(".lifecycle")
    }
}

pub(super) fn validate_local_id(id: &str) -> Result<()> {
    let mut components = Path::new(id).components();
    let valid = matches!(components.next(), Some(Component::Normal(value)) if value == id)
        && components.next().is_none();
    if id.is_empty() || !valid {
        bail!("invalid local account profile identifier")
    }
    Ok(())
}

pub(super) fn reject_symlinks(root: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(root).context("inspecting managed account profile")?;
    if metadata.file_type().is_symlink() {
        bail!("refusing to remove a managed account profile containing a symlink")
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    for entry in fs::read_dir(root).context("reading managed account profile")? {
        reject_symlinks(&entry?.path())?;
    }
    Ok(())
}
