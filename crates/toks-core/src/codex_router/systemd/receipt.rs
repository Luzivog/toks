use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::codex_router::host::{BuildId, DeploymentState, GenerationStatus};

pub(super) struct RenderedUnits {
    pub(super) coordinator: String,
    pub(super) worker: String,
    pub(super) resume: String,
    pub(super) build: BuildId,
    pub(super) contract: super::launch_contract::LaunchContract,
    pub(super) executable: PathBuf,
    pub(super) process_environment: BTreeMap<String, Option<String>>,
}

pub(super) fn render_units(executable: &Path, codex_executable: &Path) -> Result<RenderedUnits> {
    let environment = super::units::UnitEnvironment::capture();
    let artifact_root = artifact_root()?;
    render_units_at(&artifact_root, executable, codex_executable, &environment)
}

fn render_units_at(
    artifact_root: &Path,
    executable: &Path,
    codex_executable: &Path,
    environment: &super::units::UnitEnvironment,
) -> Result<RenderedUnits> {
    let contract = super::launch_contract::capture_stable(
        artifact_root,
        executable,
        codex_executable,
        environment,
    )?;
    let executable = contract.executable().to_owned();
    let worker = super::units::render_worker_unit(artifact_root)?;
    let build = contract.build_id()?;
    let coordinator =
        super::units::render_service_unit(&executable, codex_executable, &build, environment)?;
    let resume = super::resume_unit::render(&executable, codex_executable, &build, environment)?;
    let process_environment = contract.process_environment(&build);
    Ok(RenderedUnits {
        coordinator,
        worker,
        resume,
        build,
        contract,
        executable,
        process_environment,
    })
}

#[cfg(test)]
pub(super) fn render_units_test(
    artifact_root: &Path,
    executable: &Path,
    codex_executable: &Path,
    environment: &super::units::UnitEnvironment,
) -> Result<RenderedUnits> {
    render_units_at(artifact_root, executable, codex_executable, environment)
}

#[cfg(test)]
pub(in crate::codex_router) fn build_id(
    artifact_root: &Path,
    executable: &Path,
    codex_executable: &Path,
) -> Result<BuildId> {
    let environment = super::units::UnitEnvironment::capture();
    Ok(render_units_at(artifact_root, executable, codex_executable, &environment)?.build)
}

pub(super) fn failed_candidate(path: &Path, candidate: &BuildId) -> Result<bool> {
    let state = match fs::read(path) {
        Ok(bytes) => serde_json::from_slice::<DeploymentState>(&bytes)
            .context("parsing router deployment state")?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("reading router deployment state"),
    };
    state
        .validate()
        .context("validating router deployment state")?;
    let snapshot = state.snapshot();
    if snapshot.generations.iter().any(|generation| {
        generation.status == GenerationStatus::Active && &generation.build == candidate
    }) {
        return Ok(false);
    }
    Ok(snapshot
        .generations
        .iter()
        .rev()
        .find(|generation| &generation.build == candidate)
        .is_some_and(|generation| generation.status == GenerationStatus::Failed))
}

pub(in crate::codex_router) fn deployment_state_path() -> Result<PathBuf> {
    let data = toks_ingest::paths::get_data_dir().context("no local data directory")?;
    Ok(data.join("rotation/router-host.json"))
}

pub(super) fn artifact_root() -> Result<PathBuf> {
    let data = toks_ingest::paths::get_data_dir().context("no local data directory")?;
    Ok(data.join("rotation/router-artifacts"))
}

pub(super) fn active_candidate_generation(path: &Path, candidate: &BuildId) -> Result<Option<u64>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("reading router deployment state"),
    };
    let state = serde_json::from_slice::<DeploymentState>(&bytes)
        .context("parsing router deployment state")?;
    state
        .validate()
        .context("validating router deployment state")?;
    Ok(state.snapshot().generations.iter().find_map(|generation| {
        (generation.status == GenerationStatus::Active && &generation.build == candidate)
            .then_some(generation.id.get())
    }))
}
