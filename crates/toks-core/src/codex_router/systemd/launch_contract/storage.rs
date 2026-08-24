use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::codex_router::host::BuildId;

use super::{LaunchContract, StoredContract, CONTRACT_NAME};

pub(crate) fn persist(root: &Path, contract: &LaunchContract, build: &BuildId) -> Result<()> {
    contract.validate_for_root(root, build)?;
    let path = contract_path(root, build);
    super::path_guard::prepare(
        root,
        path.parent().context("launch contract has no parent")?,
    )?;
    let bytes = serde_json::to_vec_pretty(&StoredContract {
        build: build.clone(),
        contract: contract.clone(),
    })?;
    crate::storage::write_private_atomic(&path, &bytes, "router launch contract")
}

pub(super) fn load(path: &Path) -> Result<StoredContract> {
    let bytes = fs::read(path).context("reading router launch contract")?;
    serde_json::from_slice(&bytes).context("parsing router launch contract")
}

pub(super) fn load_generation(path: &Path) -> Result<StoredContract> {
    let root = generation_artifact_root(path)?;
    let stored = load(path)?;
    stored.contract.validate_for_root(root, &stored.build)?;
    Ok(stored)
}

fn generation_artifact_root(path: &Path) -> Result<&Path> {
    anyhow::ensure!(
        path.file_name().is_some_and(|name| name == CONTRACT_NAME),
        "invalid generation launch contract path"
    );
    let generation = path.parent().context("launch contract has no generation")?;
    let generations = generation
        .parent()
        .context("launch contract has no generations directory")?;
    anyhow::ensure!(
        generations
            .file_name()
            .is_some_and(|name| name == "generations"),
        "launch contract is outside generations directory"
    );
    generations
        .parent()
        .context("launch contract has no artifact root")
}

pub(crate) fn contract_path(root: &Path, build: &BuildId) -> PathBuf {
    root.join("contracts")
        .join(build.as_str())
        .join(CONTRACT_NAME)
}
