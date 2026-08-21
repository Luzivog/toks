use std::path::PathBuf;

use crate::{pricing::PricingService, scanner::ScannerSettings, UnifiedMessage};

#[derive(Debug, Clone)]
pub struct AccountingDeltaOptions {
    pub home_dir: Option<String>,
    pub use_env_roots: bool,
    pub scanner_settings: ScannerSettings,
}

impl Default for AccountingDeltaOptions {
    fn default() -> Self {
        Self {
            home_dir: None,
            use_env_roots: true,
            scanner_settings: ScannerSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SourceKey(String);

impl SourceKey {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for SourceKey {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(transparent)]
pub struct SourceRevision(String);

impl SourceRevision {
    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub(crate) fn new(value: String) -> Self {
        Self(value)
    }
}

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceCheckpoint {
    pub parser_version: u32,
    pub previous_offset: u64,
    pub committed_offset: u64,
    pub source_size: u64,
}

#[derive(Debug, Clone)]
pub struct SourceDelta {
    pub source_key: SourceKey,
    pub revision: SourceRevision,
    pub observations: Vec<UnifiedMessage>,
    #[cfg(test)]
    pub checkpoint: SourceCheckpoint,
    pub backfill_complete: bool,
    pub(crate) proposed: super::store::StoredCheckpoint,
}

#[derive(Debug, Clone, Copy)]
pub struct AccountingSource<'a> {
    pub source_key: &'a SourceKey,
    pub revision: &'a SourceRevision,
    pub observations: &'a [UnifiedMessage],
    pub backfill_complete: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AccountingAdvance {
    pub backlog: AccountingBacklog,
    pub archived_sources: usize,
}

#[derive(Debug)]
pub enum AccountingAdvanceError<E> {
    Ingest(String),
    Archive(E),
    CheckpointAfterArchive(String),
}

impl<E: std::fmt::Display> std::fmt::Display for AccountingAdvanceError<E> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Ingest(error) => write!(formatter, "accounting ingest: {error}"),
            Self::Archive(error) => write!(formatter, "archiving accounting source: {error}"),
            Self::CheckpointAfterArchive(error) => {
                write!(
                    formatter,
                    "checkpointing archived accounting source: {error}"
                )
            }
        }
    }
}

impl<E> std::error::Error for AccountingAdvanceError<E> where E: std::error::Error + 'static {}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AccountingBacklog {
    pub discovered_sources: usize,
    pub changed_sources: usize,
    pub pending_sources: usize,
    /// The persisted fair-scheduling cursor moved past a bounded batch even
    /// when none of those sources yielded an archive delta.
    pub scan_progress: bool,
}

#[cfg(test)]
#[derive(Debug, Clone, Default)]
pub struct AccountingDelta {
    pub sources: Vec<SourceDelta>,
    pub backlog: AccountingBacklog,
}

pub(crate) struct SourceCandidate {
    pub kind: SourceKind,
    pub path: PathBuf,
    pub key: SourceKey,
    pub size: u64,
    pub modified_ns: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum SourceKind {
    Codex,
    Claude,
    OpenCode,
}

pub(crate) struct CollectContext<'a> {
    pub pricing: Option<&'a PricingService>,
    pub home_dir: &'a std::path::Path,
}

pub(crate) struct ProcessedSource {
    pub delta: Option<SourceDelta>,
    pub remains_pending: bool,
}
