use anyhow::{anyhow, Context, Result};
use nix::fcntl::{Flock, FlockArg};
use serde::{Deserialize, Serialize};
use std::fs;
use std::fs::OpenOptions;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

use super::{BuildId, RetryId};

const RETRY_VERSION: u8 = 1;
const RETRY_NAME: &str = "router-host-retry.json";
const RETRY_LOCK_NAME: &str = "router-host-retry.lock";

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RetryIntent {
    version: u8,
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
        crate::rotation::write_private_atomic(
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
    let file = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .mode(0o600)
        .open(parent.join(RETRY_LOCK_NAME))?;
    let _lock = Flock::lock(file, FlockArg::LockExclusive)
        .map_err(|(_, error)| anyhow!(error))
        .context("locking router deployment retry intent")?;
    action()
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::{clear_retry_intent, load_retry_intent, request_retry};
    use crate::codex_router::host::BuildId;

    #[test]
    fn retry_intent_is_durable_and_one_shot() {
        let directory = tempdir().unwrap();
        let state = directory.path().join("router-host.json");
        let build = BuildId::new("candidate").unwrap();

        let intent = request_retry(&state, &build).unwrap();
        assert_eq!(load_retry_intent(&state).unwrap(), Some(intent.clone()));
        assert_eq!(request_retry(&state, &build).unwrap(), intent);
        assert!(clear_retry_intent(&state, &intent).unwrap());
        assert_eq!(load_retry_intent(&state).unwrap(), None);
    }

    #[test]
    fn stale_build_intent_cannot_clear_the_newer_pending_retry() {
        let directory = tempdir().unwrap();
        let state = directory.path().join("router-host.json");
        let build_a = BuildId::new("build-a").unwrap();
        let build_b = BuildId::new("build-b").unwrap();

        let stale_b = request_retry(&state, &build_b).unwrap();
        let current_a = request_retry(&state, &build_a).unwrap();

        assert!(!clear_retry_intent(&state, &stale_b).unwrap());
        assert_eq!(load_retry_intent(&state).unwrap(), Some(current_a));
    }

    #[test]
    fn stale_coordinator_cannot_clear_a_newer_nonce_for_the_same_build() {
        let directory = tempdir().unwrap();
        let state = directory.path().join("router-host.json");
        let build = BuildId::new("candidate").unwrap();
        let current = request_retry(&state, &build).unwrap();
        let stale = super::RetryIntent {
            version: super::RETRY_VERSION,
            build,
            id: crate::codex_router::host::RetryId::for_test(999),
        };

        assert!(!clear_retry_intent(&state, &stale).unwrap());
        assert_eq!(load_retry_intent(&state).unwrap(), Some(current));
    }

    #[test]
    fn version_one_intent_without_a_nonce_migrates_to_a_stable_identity() {
        let directory = tempdir().unwrap();
        let state = directory.path().join("router-host.json");
        std::fs::write(
            state.with_file_name("router-host-retry.json"),
            br#"{"version":1,"build":"candidate"}"#,
        )
        .unwrap();

        let first = load_retry_intent(&state).unwrap().unwrap();
        let second = load_retry_intent(&state).unwrap().unwrap();
        assert_eq!(first, second);
        assert_eq!(first.build, BuildId::new("candidate").unwrap());
        assert_eq!(first.id.as_str(), "legacy-v1");
    }

    #[test]
    fn retry_intent_rejects_noncanonical_persisted_ids() {
        let directory = tempdir().unwrap();
        let state = directory.path().join("router-host.json");
        for id in [
            "attacker-controlled",
            "00000000-0000-4000-8000-00000000000A",
        ] {
            std::fs::write(
                state.with_file_name("router-host-retry.json"),
                format!(r#"{{"version":1,"build":"candidate","id":"{id}"}}"#),
            )
            .unwrap();
            assert!(load_retry_intent(&state).is_err());
        }
    }
}
