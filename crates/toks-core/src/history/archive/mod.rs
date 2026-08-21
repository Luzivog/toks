//! Durable, privacy-preserving accounting facts derived from provider logs.
//!
//! Provider transcripts are inputs; missing transcripts never delete accepted facts.
mod aliases;
mod batch;
mod candidate;
mod canonical;
mod change;
mod checkpoint;
mod forgotten;
mod identity;
mod load;
mod lookup;
mod paths;
mod projection;
mod projection_load;
mod projection_migration;
mod projection_types;
mod reader;
mod resolve;
mod schema;
mod store;
#[cfg(test)]
mod test_support;
mod writer;
use anyhow::{Context, Result};
pub(super) use projection_types::{ArchiveProjection, ArchiveRollup, RollupPeriod};
pub(super) use reader::{
    load_default, load_metadata_default, refresh_default as refresh_projection_default,
};
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::path::Path;
#[cfg(test)]
#[allow(unused_imports)]
pub(super) use test_support::{
    advance_projection_at_for_test, load_projection_metadata_at_for_test,
    stream_projection_at_for_test,
};
use toks_ingest::sessions::UnifiedMessage;
pub(super) use writer::ArchiveWriter;

#[cfg(test)]
#[derive(Debug, Default)]
pub(super) struct ArchiveCapture {
    pub messages: Vec<UnifiedMessage>,
    pub captured_since_ms: Option<i64>,
    pub captured_through_ms: Option<i64>,
    pub strong_events: i64,
    pub weak_events: i64,
    pub conflicts: i64,
    pub pending_sources: usize,
}

#[derive(Clone, Copy)]
pub(super) struct SourceDelta<'a> {
    pub source_key: &'a str,
    pub revision: &'a str,
    pub observations: &'a [UnifiedMessage],
    pub backfill_complete: bool,
}

pub(super) struct ArchiveApply {
    pub projection: ArchiveProjection,
    pub changed: bool,
}

pub(super) fn forget_range_default(start_ms: i64, end_ms: i64) -> Result<usize> {
    let path = paths::default_path().context("no local data directory for usage archive")?;
    forgotten::forget_range(&path, start_ms, end_ms)
}

#[cfg(test)]
fn reconcile_at(
    path: &Path,
    observations: &[UnifiedMessage],
    observed_at_ms: i64,
) -> Result<ArchiveCapture> {
    let mut groups = BTreeMap::<String, Vec<UnifiedMessage>>::new();
    for message in observations {
        groups
            .entry(format!("{}\0{}", message.client, message.session_id))
            .or_default()
            .push(message.clone());
    }
    let mut connection = schema::open(path)?;
    for (source_key, messages) in &groups {
        let revision = batch::revision(messages)?;
        checkpoint::apply(
            &mut connection,
            SourceDelta {
                source_key,
                revision: &revision,
                observations: messages,
                backfill_complete: true,
            },
            observed_at_ms,
        )?;
    }
    touch_capture(&mut connection, observed_at_ms)?;
    projection_migration::advance(&mut connection)?;
    load::capture(&connection)
}

#[cfg(test)]
fn load_at(path: &Path) -> Result<Option<ArchiveCapture>> {
    if !path.exists() {
        return Ok(None);
    }
    let connection = schema::open(path)?;
    if !load::initialized(&connection)? {
        return Ok(None);
    }
    load::capture(&connection).map(Some)
}

#[cfg(test)]
fn touch_capture(connection: &mut rusqlite::Connection, observed_at_ms: i64) -> Result<()> {
    let transaction = connection.transaction()?;
    store::update_capture_state(&transaction, observed_at_ms)?;
    transaction.commit()?;
    Ok(())
}

#[cfg(test)]
mod claude_tests;
#[cfg(test)]
mod delta_tests;
#[cfg(test)]
mod forgotten_tests;
#[cfg(test)]
mod identity_tests;
#[cfg(test)]
mod performance_tests;
#[cfg(test)]
mod projection_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod writer_tests;
