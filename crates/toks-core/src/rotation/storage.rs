use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use super::runtime::RUNTIME_VERSION;
use super::settings::SETTINGS_VERSION;
use super::{RotationRuntime, RotationSettings};
use crate::storage::{LockMode, PrivateFileLock, StoreUpdate};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationPaths {
    settings: PathBuf,
    runtime: PathBuf,
}

impl RotationPaths {
    pub fn discover() -> Result<Self> {
        Ok(Self {
            settings: crate::paths::rotation_settings()?,
            runtime: crate::paths::rotation_runtime()?,
        })
    }

    pub fn for_data_dir(root: impl AsRef<Path>) -> Self {
        Self {
            settings: crate::paths::rotation_settings_at(root.as_ref()),
            runtime: crate::paths::rotation_runtime_at(root.as_ref()),
        }
    }

    pub fn settings(&self) -> &Path {
        &self.settings
    }

    pub fn runtime(&self) -> &Path {
        &self.runtime
    }
}

#[derive(Debug, Clone)]
pub struct RotationSettingsStore {
    path: PathBuf,
}

impl RotationSettingsStore {
    pub fn discover() -> Result<Self> {
        Ok(Self::at(RotationPaths::discover()?.settings))
    }

    pub fn for_data_dir(root: impl AsRef<Path>) -> Self {
        Self::at(RotationPaths::for_data_dir(root).settings)
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RotationSettings> {
        let Some(mut settings) = read_json::<RotationSettings>(&self.path, "rotation settings")?
        else {
            return Ok(RotationSettings::default());
        };
        let version = settings.version();
        if version != SETTINGS_VERSION {
            bail!("unsupported rotation settings version {version}");
        }
        settings.normalize();
        Ok(settings)
    }

    pub fn save(&self, settings: &RotationSettings) -> Result<()> {
        let _lock = lock_document(&self.path, "rotation settings")?;
        write_json(&self.path, settings, "rotation settings")
    }

    /// Serialize a settings read-modify-write across UI polls, actions, and
    /// other Toks processes.
    pub fn update<T>(
        &self,
        change: impl FnOnce(&mut RotationSettings) -> StoreUpdate<T>,
    ) -> Result<T> {
        let _lock = lock_document(&self.path, "rotation settings")?;
        let mut settings = self.load()?;
        let (value, changed) = change(&mut settings).into_parts();
        if changed {
            write_json(&self.path, &settings, "rotation settings")?;
        }
        Ok(value)
    }
}

#[derive(Debug, Clone)]
pub struct RotationRuntimeStore {
    path: PathBuf,
}

impl RotationRuntimeStore {
    pub fn discover() -> Result<Self> {
        Ok(Self::at(RotationPaths::discover()?.runtime))
    }

    pub fn for_data_dir(root: impl AsRef<Path>) -> Self {
        Self::at(RotationPaths::for_data_dir(root).runtime)
    }

    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn load(&self) -> Result<RotationRuntime> {
        let Some(mut runtime) = read_json::<RotationRuntime>(&self.path, "rotation runtime")?
        else {
            return Ok(RotationRuntime::default());
        };
        if runtime.version() != RUNTIME_VERSION {
            bail!("unsupported rotation runtime version {}", runtime.version());
        }
        runtime.normalize()?;
        Ok(runtime)
    }

    #[cfg(test)]
    pub(crate) fn save(&self, runtime: &RotationRuntime) -> Result<()> {
        let _lock = lock_document(&self.path, "rotation runtime")?;
        self.save_unlocked(runtime)
    }

    fn save_unlocked(&self, runtime: &RotationRuntime) -> Result<()> {
        runtime.validate()?;
        write_json(&self.path, runtime, "rotation runtime")
    }

    /// Serializes a read-modify-write transaction across router generations.
    pub(crate) fn update<T>(
        &self,
        change: impl FnOnce(&mut RotationRuntime) -> StoreUpdate<T>,
    ) -> Result<T> {
        let _lock = lock_document(&self.path, "rotation runtime")?;
        let mut runtime = self.load()?;
        let (value, changed) = change(&mut runtime).into_parts();
        if changed {
            runtime.validate()?;
            self.save_unlocked(&runtime)?;
        }
        Ok(value)
    }
}

pub(super) fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {label}")),
    };
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {label}"))
        .map(Some)
}

pub(super) fn write_json(path: &Path, value: &impl Serialize, label: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    crate::storage::restrict_directory(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    crate::storage::write_private_atomic(path, &bytes, label)
}

pub(super) fn lock_document(path: &Path, label: &str) -> Result<PrivateFileLock> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    crate::storage::restrict_directory(parent)?;
    let mut name = path
        .file_name()
        .with_context(|| format!("{label} path has no file name"))?
        .to_os_string();
    name.push(".lock");
    crate::storage::lock_private(&parent.join(name), label, LockMode::Blocking)
}
