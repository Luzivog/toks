use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{de::DeserializeOwned, Serialize};

use super::runtime::RUNTIME_VERSION;
use super::settings::SETTINGS_VERSION;
use super::{RotationRuntime, RotationSettings};

mod atomic;
use atomic::restrict_directory;
pub(crate) use atomic::write_private_atomic;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RotationPaths {
    settings: PathBuf,
    runtime: PathBuf,
}

impl RotationPaths {
    pub fn discover() -> Result<Self> {
        let root = toks_ingest::paths::get_data_dir().context("no local data directory")?;
        Ok(Self::for_data_dir(root))
    }

    pub fn for_data_dir(root: impl AsRef<Path>) -> Self {
        let directory = root.as_ref().join("rotation");
        Self {
            settings: directory.join("settings.json"),
            runtime: directory.join("runtime.json"),
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
        write_json(&self.path, settings, "rotation settings")
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
        runtime.normalize();
        Ok(runtime)
    }

    pub fn save(&self, runtime: &RotationRuntime) -> Result<()> {
        write_json(&self.path, runtime, "rotation runtime")
    }
}

fn read_json<T: DeserializeOwned>(path: &Path, label: &str) -> Result<Option<T>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).with_context(|| format!("reading {label}")),
    };
    serde_json::from_slice(&bytes)
        .with_context(|| format!("parsing {label}"))
        .map(Some)
}

fn write_json(path: &Path, value: &impl Serialize, label: &str) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("{label} path has no parent"))?;
    fs::create_dir_all(parent).with_context(|| format!("creating {label} directory"))?;
    restrict_directory(parent)?;
    let bytes = serde_json::to_vec_pretty(value)?;
    write_private_atomic(path, &bytes, label)
}
