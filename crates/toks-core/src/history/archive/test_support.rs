use std::path::Path;

use anyhow::Result;

use super::{
    load, projection_load, projection_migration, schema, ArchiveProjection, ArchiveRollup,
};

pub(in crate::history) fn load_projection_metadata_at_for_test(
    path: &Path,
) -> Result<Option<ArchiveProjection>> {
    let Some(connection) = open_at(path)? else {
        return Ok(None);
    };
    projection_load::metadata(&connection).map(Some)
}

pub(in crate::history) fn advance_projection_at_for_test(
    path: &Path,
    visit: impl FnMut(&ArchiveRollup),
) -> Result<Option<ArchiveProjection>> {
    let Some(mut connection) = open_at(path)? else {
        return Ok(None);
    };
    projection_migration::advance(&mut connection)?;
    projection_load::stream(&connection, visit).map(Some)
}

pub(in crate::history) fn stream_projection_at_for_test(
    path: &Path,
    visit: impl FnMut(&ArchiveRollup),
) -> Result<Option<ArchiveProjection>> {
    let Some(connection) = open_at(path)? else {
        return Ok(None);
    };
    projection_load::stream(&connection, visit).map(Some)
}

fn open_at(path: &Path) -> Result<Option<rusqlite::Connection>> {
    if !path.exists() {
        return Ok(None);
    }
    let connection = schema::open(path)?;
    if !load::initialized(&connection)? {
        return Ok(None);
    }
    Ok(Some(connection))
}
