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

fn environment_at(path: &std::path::Path, account: &AccountId) -> Result<Option<String>> {
    Ok(load(path)?.environments.get(account).cloned())
}

fn remember_at(path: &std::path::Path, account: &AccountId, environment: &str) -> Result<()> {
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
    crate::rotation::write_private_atomic(path, &bytes, "Remote Control state")
}

fn path() -> Result<PathBuf> {
    toks_ingest::paths::get_data_dir()
        .map(|root| root.join("remote-control.json"))
        .context("no local data directory")
}

#[cfg(test)]
mod tests {
    use super::{environment_at, remember_at};
    use crate::accounts::AccountId;

    #[test]
    fn environments_survive_restart_and_stay_scoped_to_the_control_account() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("remote-control.json");
        let first = AccountId::new("first");
        let second = AccountId::new("second");
        remember_at(&path, &first, "environment-a").unwrap();
        remember_at(&path, &second, "environment-b").unwrap();
        assert_eq!(
            environment_at(&path, &first).unwrap().as_deref(),
            Some("environment-a")
        );
        assert_eq!(
            environment_at(&path, &second).unwrap().as_deref(),
            Some("environment-b")
        );
    }
}
