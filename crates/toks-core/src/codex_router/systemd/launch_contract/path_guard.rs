use std::fs;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context, Result};

pub(super) fn prepare(root: &Path, directory: &Path) -> Result<()> {
    ensure_root(root)?;
    let relative = directory
        .strip_prefix(root)
        .context("router artifact destination is outside router artifact root")?;
    anyhow::ensure!(
        relative
            .components()
            .all(|component| matches!(component, Component::Normal(_))),
        "router artifact destination is outside router artifact root"
    );
    let canonical_root = root
        .canonicalize()
        .context("canonicalizing router artifact root")?;
    let mut current = root.to_path_buf();
    for component in relative.components() {
        current.push(component.as_os_str());
        ensure_directory(&current)?;
    }
    let canonical = current
        .canonicalize()
        .context("canonicalizing router artifact destination")?;
    anyhow::ensure!(
        canonical.starts_with(canonical_root),
        "router artifact destination escaped router artifact root"
    );
    Ok(())
}

fn ensure_root(root: &Path) -> Result<()> {
    anyhow::ensure!(root.is_absolute(), "router artifact root is not absolute");
    let mut current = PathBuf::new();
    for component in root.components() {
        match component {
            Component::RootDir => current.push(Path::new("/")),
            Component::Normal(value) => current.push(value),
            _ => anyhow::bail!("router artifact root has an invalid component"),
        }
        ensure_directory(&current)?;
    }
    Ok(())
}

fn ensure_directory(path: &PathBuf) -> Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => validate_directory(path, &metadata),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            match fs::create_dir(path) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error).context("creating router artifact directory"),
            }
            let metadata = fs::symlink_metadata(path)?;
            validate_directory(path, &metadata)
        }
        Err(error) => Err(error).context("inspecting router artifact directory"),
    }
}

fn validate_directory(path: &Path, metadata: &fs::Metadata) -> Result<()> {
    anyhow::ensure!(
        !metadata.file_type().is_symlink(),
        "router artifact ancestor is a symlink: {}",
        path.display()
    );
    anyhow::ensure!(
        metadata.is_dir(),
        "router artifact ancestor is not a directory: {}",
        path.display()
    );
    Ok(())
}
