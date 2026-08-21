use std::fs;
use std::path::PathBuf;

use crate::sessions::codex::CodexParseState;

use super::types::{SourceKey, SourceKind};

mod database;
mod key;
mod metadata;
mod migration;
mod scheduling;
mod schema;
mod wire;

#[cfg(test)]
mod tests;

const KEY_BYTES: usize = 32;
const KEY_FILE: &str = "accounting-source.key";
const LOCK_FILE: &str = "accounting-checkpoints.lock";
const DATABASE_FILE: &str = "accounting-checkpoints-v2.sqlite";
const LEGACY_FILE: &str = "accounting-checkpoints-v1.json";

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct SampleDigest {
    pub offset: u64,
    pub len: u64,
    pub hash: [u8; 32],
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub(crate) struct StoredCheckpoint {
    pub kind: SourceKind,
    pub parser_version: u32,
    pub committed_offset: u64,
    pub source_size: u64,
    pub modified_ns: u64,
    pub content_hash: [u8; 32],
    pub prefix_samples: Vec<SampleDigest>,
    pub codex_state: Option<CodexParseState>,
}

#[derive(Debug)]
pub(crate) struct CheckpointSummary {
    pub kind: SourceKind,
    pub parser_version: u32,
    pub committed_offset: u64,
    pub source_size: u64,
    pub modified_ns: u64,
    pub prefix_samples: Vec<SampleDigest>,
}

pub(crate) struct CheckpointStore {
    connection: rusqlite::Connection,
    _lock: fs::File,
    key: [u8; KEY_BYTES],
}

impl CheckpointStore {
    pub fn open(directory: PathBuf) -> Result<Self, String> {
        key::ensure_private_directory(&directory)?;
        let lock = super::lock::acquire(&directory.join(LOCK_FILE))?;
        let key = key::load_or_create(&directory.join(KEY_FILE))?;
        let connection = migration::open(&directory)?;
        Ok(Self {
            connection,
            _lock: lock,
            key,
        })
    }

    pub fn key(&self) -> &[u8; KEY_BYTES] {
        &self.key
    }

    pub fn get(&self, source: &SourceKey) -> Result<Option<StoredCheckpoint>, String> {
        database::load_checkpoint(&self.connection, source)
    }

    pub fn summary(&self, source: &SourceKey) -> Result<Option<CheckpointSummary>, String> {
        database::load_summary(&self.connection, source)
    }

    #[cfg(test)]
    pub fn commit<'a>(
        &mut self,
        checkpoints: impl Iterator<Item = (&'a SourceKey, &'a StoredCheckpoint)>,
    ) -> Result<(), String> {
        database::commit(&mut self.connection, checkpoints)
    }

    pub fn acknowledge(
        &mut self,
        source: &SourceKey,
        checkpoint: Option<&StoredCheckpoint>,
    ) -> Result<(), String> {
        database::acknowledge(&mut self.connection, source, checkpoint)
    }
}
