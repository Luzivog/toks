use super::units::UnitEnvironment;
use crate::codex_router::host::BuildId;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
mod artifact;
mod generation;
mod path_guard;
mod storage;
pub(super) use artifact::capture as capture_stable;
pub(crate) use generation::stage;
pub(super) use storage::{contract_path, persist};
const CONTRACT_VERSION: u8 = 1;
pub(super) const CONTRACT_NAME: &str = "launch.json";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct LaunchContract {
    version: u8,
    executable: PathBuf,
    environment: BTreeMap<String, Option<String>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredContract {
    build: BuildId,
    contract: LaunchContract,
}
pub(in crate::codex_router) struct WorkerProcessContract {
    pub(in crate::codex_router) executable: PathBuf,
    pub(in crate::codex_router) environment: BTreeMap<String, Option<String>>,
}

pub(in crate::codex_router) fn coordinator_process_contract(
    root: &Path,
    build: &BuildId,
) -> Result<WorkerProcessContract> {
    let stored = storage::load(&storage::contract_path(root, build))?;
    anyhow::ensure!(&stored.build == build, "launch contract identity mismatch");
    stored.contract.validate_for_root(root, build)?;
    Ok(WorkerProcessContract {
        executable: stored.contract.executable.clone(),
        environment: stored.contract.process_environment(&stored.build),
    })
}

impl LaunchContract {
    pub(super) fn capture(
        executable: &Path,
        codex_executable: &Path,
        inherited: &UnitEnvironment,
    ) -> Result<Self> {
        let executable = executable
            .canonicalize()
            .context("canonicalizing router candidate")?;
        let mut environment = inherited.values();
        environment.insert(
            "TOKS_CODEX_BIN".into(),
            Some(codex_executable.display().to_string()),
        );
        Ok(Self {
            version: CONTRACT_VERSION,
            executable,
            environment,
        })
    }

    pub(super) fn build_id(&self) -> Result<BuildId> {
        let bytes = fs::read(&self.executable).context("reading router candidate")?;
        let mut digest = Sha256::new();
        digest.update(b"toks-router-launch-v2\0");
        digest.update((bytes.len() as u64).to_le_bytes());
        digest.update(bytes);
        digest.update(serde_json::to_vec(self)?);
        BuildId::new(format!("{:x}", digest.finalize())).map_err(Into::into)
    }

    pub(super) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(super) fn process_environment(&self, build: &BuildId) -> BTreeMap<String, Option<String>> {
        let mut environment = self.environment.clone();
        environment.insert(
            "TOKS_ROUTER_BUILD_ID".into(),
            Some(build.as_str().to_owned()),
        );
        environment
    }

    fn validate(&self, expected: &BuildId) -> Result<()> {
        anyhow::ensure!(
            self.version == CONTRACT_VERSION,
            "unsupported launch contract"
        );
        anyhow::ensure!(
            &self.build_id()? == expected,
            "launch contract build mismatch"
        );
        Ok(())
    }

    fn validate_for_root(&self, root: &Path, expected: &BuildId) -> Result<()> {
        let executable = self
            .executable
            .canonicalize()
            .context("canonicalizing router executable artifact")?;
        anyhow::ensure!(
            executable == self.executable,
            "launch contract executable path is not canonical"
        );
        anyhow::ensure!(
            executable.starts_with(root.join("executables")),
            "launch contract executable is outside router executable artifact root"
        );
        let metadata = fs::symlink_metadata(&executable)?;
        anyhow::ensure!(
            metadata.file_type().is_file(),
            "launch contract executable is not a regular file"
        );
        anyhow::ensure!(
            metadata.permissions().mode() & 0o111 == 0o111,
            "launch contract executable is not executable"
        );
        self.validate(expected)
    }
}

pub(crate) fn launch(path: &Path, generation: u64) -> Result<()> {
    let stored = storage::load_generation(path)?;
    let running = std::env::current_exe()?.canonicalize()?;
    anyhow::ensure!(
        running == stored.contract.executable,
        "generation executable does not match its launch contract"
    );
    let mut command = worker_command(stored.contract, &stored.build, generation);
    Err(command.exec()).context("launching router worker generation")
}

fn worker_command(
    contract: LaunchContract,
    build: &BuildId,
    generation: u64,
) -> std::process::Command {
    let mut command = std::process::Command::new(&contract.executable);
    command.args(["worker", &generation.to_string()]);
    command.env_clear();
    for (name, value) in contract.process_environment(build) {
        if let Some(value) = value {
            command.env(name, value);
        }
    }
    command
}

pub(in crate::codex_router) fn worker_process_contract(
    path: &Path,
) -> Result<WorkerProcessContract> {
    let stored = storage::load_generation(path)?;
    Ok(WorkerProcessContract {
        executable: stored.contract.executable.clone(),
        environment: stored.contract.process_environment(&stored.build),
    })
}

#[cfg(test)]
pub(super) fn inspect(path: &Path) -> Result<(BuildId, PathBuf, BTreeMap<String, Option<String>>)> {
    let stored = storage::load_generation(path)?;
    Ok((
        stored.build,
        stored.contract.executable,
        stored.contract.environment,
    ))
}

#[cfg(test)]
pub(super) fn command_for_test(path: &Path, generation: u64) -> Result<std::process::Command> {
    let stored = storage::load_generation(path)?;
    Ok(worker_command(stored.contract, &stored.build, generation))
}

#[cfg(test)]
pub(crate) fn persist_test(
    artifact_root: &Path,
    executable: &Path,
    codex_executable: &Path,
    environment: &UnitEnvironment,
) -> Result<BuildId> {
    let contract = capture_stable(artifact_root, executable, codex_executable, environment)?;
    let build = contract.build_id()?;
    persist(artifact_root, &contract, &build)?;
    Ok(build)
}
