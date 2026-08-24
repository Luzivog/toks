use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::rotation::{ThreadId, UnixMillis, WaitingId};

mod attempt;
pub(in crate::codex_router::resume) use attempt::{
    ResumeAttempt, ResumePhase, ResumeTerminalState,
};

const VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct ResumeState {
    version: u8,
    pub(super) attempts: BTreeMap<ThreadId, ResumeAttempt>,
    pub(super) retry_after: BTreeMap<WaitingId, UnixMillis>,
}

impl Default for ResumeState {
    fn default() -> Self {
        Self {
            version: VERSION,
            attempts: BTreeMap::new(),
            retry_after: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct ResumeStore {
    path: PathBuf,
    outcomes: PathBuf,
}

impl ResumeStore {
    pub(super) fn discover() -> Result<Self> {
        let root = toks_ingest::paths::get_data_dir().context("no local data directory")?;
        Ok(Self::for_data_dir(root))
    }

    pub(super) fn for_data_dir(root: impl AsRef<Path>) -> Self {
        let directory = root.as_ref().join("rotation");
        Self {
            path: directory.join("resume-state.json"),
            outcomes: directory.join("resume-outcomes"),
        }
    }

    pub(super) fn load(&self) -> Result<ResumeState> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ResumeState::default());
            }
            Err(error) => return Err(error).context("reading resume state"),
        };
        let state: ResumeState = serde_json::from_slice(&bytes).context("parsing resume state")?;
        anyhow::ensure!(state.version == VERSION, "unsupported resume state version");
        state.validate()?;
        Ok(state)
    }

    pub(super) fn save(&self, state: &ResumeState) -> Result<()> {
        crate::rotation::write_private_atomic(
            &self.path,
            &serde_json::to_vec_pretty(state)?,
            "resume state",
        )
    }

    pub(super) fn outcome(&self, attempt: &str) -> Result<Option<bool>> {
        validate_attempt_id(attempt)?;
        match fs::read(self.outcome_path(attempt)) {
            Ok(bytes) => Ok(Some(serde_json::from_slice::<Outcome>(&bytes)?.success)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error).context("reading resume outcome"),
        }
    }

    pub(super) fn record_outcome(&self, attempt: &str, success: bool) -> Result<()> {
        validate_attempt_id(attempt)?;
        crate::rotation::write_private_atomic(
            &self.outcome_path(attempt),
            &serde_json::to_vec(&Outcome { success })?,
            "resume outcome",
        )
    }

    pub(super) fn remove_outcome(&self, attempt: &str) -> Result<()> {
        match fs::remove_file(self.outcome_path(attempt)) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error).context("removing resume outcome"),
        }
    }

    pub(super) fn acquire_supervisor_lock(&self) -> Result<File> {
        let parent = self.path.parent().context("resume state has no parent")?;
        fs::create_dir_all(parent)?;
        let lock = private_file(parent.join("resume-supervisor.lock"))?;
        lock.try_lock()
            .context("resume supervisor is already running")?;
        Ok(lock)
    }

    fn outcome_path(&self, attempt: &str) -> PathBuf {
        self.outcomes.join(format!("{attempt}.json"))
    }
}

impl ResumeState {
    fn validate(&self) -> Result<()> {
        let mut attempt_ids = BTreeSet::new();
        for (thread, attempt) in &self.attempts {
            attempt.validate(thread)?;
            anyhow::ensure!(
                attempt_ids.insert(&attempt.id),
                "duplicate resume attempt id {}",
                attempt.id
            );
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize)]
struct Outcome {
    success: bool,
}

pub(super) fn validate_attempt_id(attempt: &str) -> Result<()> {
    let parsed = uuid::Uuid::parse_str(attempt).context("invalid resume attempt id")?;
    anyhow::ensure!(
        parsed.to_string() == attempt,
        "non-canonical resume attempt id"
    );
    Ok(())
}

fn private_file(path: PathBuf) -> Result<File> {
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(Into::into)
}
