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
mod resolve;
mod schema;
mod store;
#[cfg(test)]
mod test_support;

#[cfg(test)]
use std::collections::BTreeMap;
use std::path::Path;

use anyhow::{Context, Result};
use tokscope_ingest::sessions::UnifiedMessage;

pub(super) use projection_types::{ArchiveProjection, ArchiveRollup, RollupPeriod};
#[cfg(test)]
#[allow(unused_imports)]
pub(super) use test_support::{
    advance_projection_at_for_test, apply_sources_at_for_test, load_projection_at_for_test,
};

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
}

pub(super) fn apply_sources_default(
    deltas: &[SourceDelta<'_>],
    observed_at_ms: i64,
) -> Result<ArchiveApply> {
    let path = paths::default_path().context("no local data directory for usage archive")?;
    apply_sources_at(&path, deltas, observed_at_ms)
}

pub(super) fn load_default() -> Result<Option<ArchiveProjection>> {
    let Some(path) = paths::default_path() else {
        return Ok(None);
    };
    load_projection_at(&path)
}

pub(super) fn refresh_projection_default(observed_at_ms: i64) -> Result<Option<ArchiveProjection>> {
    let Some(path) = paths::default_path() else {
        return Ok(None);
    };
    if !path.exists() {
        return Ok(None);
    }
    let mut connection = schema::open(&path)?;
    if !load::initialized(&connection)? {
        return Ok(None);
    }
    projection_migration::advance(&mut connection)?;
    let mut projection = projection_load::load(&connection)?;
    projection.captured_through_ms = Some(
        projection
            .captured_through_ms
            .unwrap_or(observed_at_ms)
            .max(observed_at_ms),
    );
    Ok(Some(projection))
}

pub(super) fn forget_range_default(start_ms: i64, end_ms: i64) -> Result<usize> {
    let path = paths::default_path().context("no local data directory for usage archive")?;
    forgotten::forget_range(&path, start_ms, end_ms)
}

fn apply_sources_at(
    path: &Path,
    deltas: &[SourceDelta<'_>],
    observed_at_ms: i64,
) -> Result<ArchiveApply> {
    let mut connection = schema::open(path)?;
    for delta in deltas {
        checkpoint::apply(&mut connection, *delta, observed_at_ms)?;
    }
    projection_migration::advance(&mut connection)?;
    Ok(ArchiveApply {
        projection: projection_load::load(&connection)?,
    })
}

fn load_projection_at(path: &Path) -> Result<Option<ArchiveProjection>> {
    if !path.exists() {
        return Ok(None);
    }
    let connection = schema::open(path)?;
    if !load::initialized(&connection)? {
        return Ok(None);
    }
    projection_load::load(&connection).map(Some)
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
