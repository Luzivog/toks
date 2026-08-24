use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use super::{BuildId, RetryId};
use crate::storage::LockMode;

pub(super) const RETRY_VERSION: u8 = 1;
const RETRY_NAME: &str = "router-host-retry.json";
const RETRY_LOCK_NAME: &str = "router-host-retry.lock";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetryIntent {
    pub(super) version: u8,
    pub(crate) build: BuildId,
    #[serde(default)]
    pub(crate) id: RetryId,
}

pub(crate) fn request_retry(state: &Path, build: &BuildId) -> Result<RetryIntent> {
    with_retry_lock(state, || {
        if let Some(existing) = load_retry_intent(state)? {
            if &existing.build == build {
                return Ok(existing);
            }
        }
        let intent = RetryIntent {
            version: RETRY_VERSION,
            build: build.clone(),
            id: RetryId::fresh(),
        };
        crate::storage::write_private_atomic(
            &retry_path(state),
            &serde_json::to_vec(&intent)?,
            "router deployment retry intent",
        )?;
        Ok(intent)
    })
}

pub(crate) fn load_retry_intent(state: &Path) -> Result<Option<RetryIntent>> {
    let bytes = match fs::read(retry_path(state)) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("reading router retry intent"),
    };
    let intent: RetryIntent =
        serde_json::from_slice(&bytes).context("parsing router retry intent")?;
    anyhow::ensure!(
        intent.version == RETRY_VERSION,
        "unsupported router retry intent version"
    );
    RetryId::new(intent.id.as_str()).context("invalid router retry intent id")?;
    Ok(Some(intent))
}

pub(crate) fn clear_retry_intent(state: &Path, expected: &RetryIntent) -> Result<bool> {
    with_retry_lock(state, || {
        if load_retry_intent(state)?.as_ref() != Some(expected) {
            return Ok(false);
        }
        match fs::remove_file(retry_path(state)) {
            Ok(()) => Ok(true),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
            Err(error) => Err(error).context("clearing router retry intent"),
        }
    })
}

fn retry_path(state: &Path) -> PathBuf {
    state.with_file_name(RETRY_NAME)
}

fn with_retry_lock<T>(state: &Path, action: impl FnOnce() -> Result<T>) -> Result<T> {
    let parent = state.parent().context("router state has no parent")?;
    fs::create_dir_all(parent)?;
    let _lock = crate::storage::lock_private(
        &parent.join(RETRY_LOCK_NAME),
        "router deployment retry intent",
        LockMode::Blocking,
    )
    .context("locking router deployment retry intent")?;
    action()
}
