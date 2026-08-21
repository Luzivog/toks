use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

use super::{
    load, paths, projection_load, projection_migration, schema, ArchiveProjection, ArchiveRollup,
};

pub(in crate::history) fn load_default(
    visit: impl FnMut(&ArchiveRollup),
) -> Result<Option<ArchiveProjection>> {
    let Some(connection) = open_default()? else {
        return Ok(None);
    };
    projection_load::stream(&connection, visit).map(Some)
}

pub(in crate::history) fn load_metadata_default() -> Result<Option<ArchiveProjection>> {
    let Some(connection) = open_default()? else {
        return Ok(None);
    };
    projection_load::metadata(&connection).map(Some)
}

pub(in crate::history) fn refresh_default(
    observed_at_ms: i64,
    visit: impl FnMut(&ArchiveRollup),
) -> Result<Option<ArchiveProjection>> {
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
    let mut projection = projection_load::stream(&connection, visit)?;
    projection.captured_through_ms = Some(
        projection
            .captured_through_ms
            .unwrap_or(observed_at_ms)
            .max(observed_at_ms),
    );
    Ok(Some(projection))
}

fn open_default() -> Result<Option<Connection>> {
    let Some(path) = paths::default_path() else {
        return Ok(None);
    };
    open_initialized(&path)
}

fn open_initialized(path: &Path) -> Result<Option<Connection>> {
    if !path.exists() {
        return Ok(None);
    }
    let connection = schema::open(path)?;
    if !load::initialized(&connection)? {
        return Ok(None);
    }
    Ok(Some(connection))
}
