use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::codex_router::host::BuildId;

use super::CONTRACT_NAME;

pub(crate) fn stage(root: &Path, generation: &Path, expected: &BuildId) -> Result<PathBuf> {
    let contract = super::contract_path(root, expected);
    super::path_guard::prepare(
        root,
        contract.parent().context("launch contract has no parent")?,
    )?;
    let stored = super::storage::load(&contract)?;
    anyhow::ensure!(
        &stored.build == expected,
        "launch contract identity mismatch"
    );
    stored.contract.validate_for_root(root, expected)?;
    anyhow::ensure!(
        generation.starts_with(root.join("generations")),
        "router artifact destination is outside router artifact root"
    );
    super::path_guard::prepare(root, generation)?;
    let destination = generation.join("toks-router");
    stage_symlink(&stored.contract.executable, &destination)?;
    let bytes = serde_json::to_vec_pretty(&stored)?;
    crate::storage::write_private_atomic(
        &generation.join(CONTRACT_NAME),
        &bytes,
        "generation launch contract",
    )?;
    Ok(destination)
}

fn stage_symlink(source: &Path, destination: &Path) -> Result<()> {
    if let Ok(found) = destination.canonicalize() {
        anyhow::ensure!(found == source, "generation points at a different build");
        return Ok(());
    }
    let temporary = crate::storage::unique_temp_path(destination)?;
    let result = (|| {
        std::os::unix::fs::symlink(source, &temporary)?;
        fs::rename(&temporary, destination)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}
