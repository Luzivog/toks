use std::path::Path;

use anyhow::{Context, Result};
use rusqlite::Connection;

use super::{
    checkpoint, paths, projection_load, projection_migration, schema, ArchiveApply, ArchiveRollup,
    SourceDelta,
};

/// Applies source deltas without retaining observations from earlier sources.
pub(in crate::history) struct ArchiveWriter {
    connection: Connection,
    observed_at_ms: i64,
    changed: bool,
}

impl ArchiveWriter {
    pub(in crate::history) fn open_default(observed_at_ms: i64) -> Result<Self> {
        let path = paths::default_path().context("no local data directory for usage archive")?;
        Self::open_at(&path, observed_at_ms)
    }

    pub(in crate::history) fn open_at(path: &Path, observed_at_ms: i64) -> Result<Self> {
        Ok(Self {
            connection: schema::open(path)?,
            observed_at_ms,
            changed: false,
        })
    }

    /// Commits one source atomically and releases its observations on return.
    pub(in crate::history) fn apply(&mut self, delta: SourceDelta<'_>) -> Result<bool> {
        let changes_before = self.connection.total_changes();
        checkpoint::apply(&mut self.connection, delta, self.observed_at_ms)?;
        let changed = self.connection.total_changes() > changes_before;
        self.changed |= changed;
        Ok(changed)
    }

    pub(in crate::history) fn finish(
        mut self,
        visit: impl FnMut(&ArchiveRollup),
    ) -> Result<ArchiveApply> {
        projection_migration::advance(&mut self.connection)?;
        let mut projection = projection_load::stream(&self.connection, visit)?;
        projection.captured_through_ms = Some(
            projection
                .captured_through_ms
                .unwrap_or(self.observed_at_ms)
                .max(self.observed_at_ms),
        );
        Ok(ArchiveApply {
            projection,
            changed: self.changed,
        })
    }
}
