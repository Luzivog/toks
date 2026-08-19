use std::path::Path;

use anyhow::Result;

use super::{
    apply_sources_at, load_projection_at, projection_load, projection_migration, schema,
    ArchiveApply, ArchiveProjection, SourceDelta,
};

pub(in crate::history) fn apply_sources_at_for_test(
    path: &Path,
    deltas: &[SourceDelta<'_>],
    observed_at_ms: i64,
) -> Result<ArchiveApply> {
    apply_sources_at(path, deltas, observed_at_ms)
}

pub(in crate::history) fn load_projection_at_for_test(
    path: &Path,
) -> Result<Option<ArchiveProjection>> {
    load_projection_at(path)
}

pub(in crate::history) fn advance_projection_at_for_test(
    path: &Path,
) -> Result<Option<ArchiveProjection>> {
    if !path.exists() {
        return Ok(None);
    }
    let mut connection = schema::open(path)?;
    projection_migration::advance(&mut connection)?;
    projection_load::load(&connection).map(Some)
}
