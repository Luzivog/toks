use std::{collections::BTreeMap, fs, path::PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::accounts::AccountId;

const VERSION: u8 = 1;

#[derive(Default, Deserialize, Serialize)]
struct State {
    version: u8,
    environments: BTreeMap<AccountId, String>,
}

pub(super) fn environment(account: &AccountId) -> Result<Option<String>> {
    environment_at(&path()?, account)
}

pub(super) fn remember(account: &AccountId, environment: &str) -> Result<()> {
    remember_at(&path()?, account, environment)
}

pub(super) fn environment_at(
    path: &std::path::Path,
    account: &AccountId,
) -> Result<Option<String>> {
    Ok(load(path)?.environments.get(account).cloned())
}

pub(super) fn remember_at(
    path: &std::path::Path,
    account: &AccountId,
    environment: &str,
) -> Result<()> {
    let mut state = load(path)?;
    if state.environments.get(account).map(String::as_str) == Some(environment) {
        return Ok(());
    }
    state
        .environments
        .insert(account.clone(), environment.into());
    save(path, &state)
}

fn load(path: &std::path::Path) -> Result<State> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(State {
                version: VERSION,
                ..Default::default()
            });
        }
        Err(error) => return Err(error).context("reading Remote Control state"),
    };
    let state: State = serde_json::from_slice(&bytes).context("parsing Remote Control state")?;
    if state.version != VERSION {
        bail!("unsupported Remote Control state version {}", state.version);
    }
    Ok(state)
}

fn save(path: &std::path::Path, state: &State) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(state)?;
    crate::storage::write_private_atomic(path, &bytes, "Remote Control state")
}

fn path() -> Result<PathBuf> {
    crate::paths::remote_control_store()
}
