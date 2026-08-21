use std::fmt;
use std::fs::{self, File};
use std::io::BufReader;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, TransactionBehavior};
use serde::de::{DeserializeSeed, IgnoredAny, MapAccess, Visitor};

use super::{database, key, metadata, schema, StoredCheckpoint, DATABASE_FILE, LEGACY_FILE};
use crate::accounting_delta::SourceKey;

const LEGACY_VERSION: u32 = 1;

pub(super) fn open(directory: &Path) -> Result<Connection, String> {
    let database_path = directory.join(DATABASE_FILE);
    let legacy_path = directory.join(LEGACY_FILE);
    let mut connection = open_connection(&database_path)?;
    schema::initialize(&connection)?;
    if !legacy_path.exists() {
        key::sync_parent(&database_path)?;
        return Ok(connection);
    }

    let expected_sources = if metadata::legacy_imported(&connection)? {
        metadata::source_count(&connection)?
    } else {
        import(&mut connection, &legacy_path)?
    };
    drop(connection);

    let reopened = open_connection(&database_path)?;
    schema::validate(&reopened)?;
    if !metadata::legacy_imported(&reopened)?
        || metadata::source_count(&reopened)? != expected_sources
    {
        return Err("accounting checkpoint migration validation failed".to_string());
    }
    rename_legacy(&legacy_path)?;
    key::sync_parent(&database_path)?;
    Ok(reopened)
}

fn open_connection(path: &Path) -> Result<Connection, String> {
    let connection = Connection::open(path).map_err(|error| error.to_string())?;
    key::make_private_file(path)?;
    schema::configure(&connection)?;
    Ok(connection)
}

fn import(connection: &mut Connection, path: &Path) -> Result<u64, String> {
    if metadata::source_count(connection)? != 0 {
        return Err("cannot import legacy checkpoints into a nonempty database".to_string());
    }
    let transaction = connection
        .transaction_with_behavior(TransactionBehavior::Immediate)
        .map_err(|error| error.to_string())?;
    let file = File::open(path).map_err(|error| error.to_string())?;
    let mut deserializer = serde_json::Deserializer::from_reader(BufReader::new(file));
    let imported = LegacySeed {
        connection: &transaction,
    }
    .deserialize(&mut deserializer)
    .map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())?;
    if imported.schema_version != LEGACY_VERSION {
        return Err("unsupported accounting checkpoint schema".to_string());
    }
    if let Some(cursor) = imported.rotation_cursor {
        transaction
            .execute(
                "UPDATE meta SET rotation_cursor = ?1 WHERE singleton = 1",
                [cursor],
            )
            .map_err(|error| error.to_string())?;
    }
    metadata::mark_legacy_imported(&transaction)?;
    transaction.commit().map_err(|error| error.to_string())?;
    Ok(imported.sources)
}

struct LegacySeed<'a> {
    connection: &'a Connection,
}

struct ImportedLegacy {
    schema_version: u32,
    sources: u64,
    rotation_cursor: Option<String>,
}

impl<'de> DeserializeSeed<'de> for LegacySeed<'_> {
    type Value = ImportedLegacy;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(LegacyVisitor {
            connection: self.connection,
        })
    }
}

struct LegacyVisitor<'a> {
    connection: &'a Connection,
}

impl<'de> Visitor<'de> for LegacyVisitor<'_> {
    type Value = ImportedLegacy;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a legacy accounting checkpoint object")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut schema_version = LEGACY_VERSION;
        let mut sources = 0;
        let mut rotation_cursor = None;
        while let Some(field) = map.next_key::<String>()? {
            match field.as_str() {
                "schema_version" => schema_version = map.next_value()?,
                "sources" => {
                    sources = map.next_value_seed(SourcesSeed {
                        connection: self.connection,
                    })?;
                }
                "rotation_cursor" => rotation_cursor = map.next_value()?,
                _ => {
                    map.next_value::<IgnoredAny>()?;
                }
            }
        }
        Ok(ImportedLegacy {
            schema_version,
            sources,
            rotation_cursor,
        })
    }
}

struct SourcesSeed<'a> {
    connection: &'a Connection,
}

impl<'de> DeserializeSeed<'de> for SourcesSeed<'_> {
    type Value = u64;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_map(SourcesVisitor {
            connection: self.connection,
        })
    }
}

struct SourcesVisitor<'a> {
    connection: &'a Connection,
}

impl<'de> Visitor<'de> for SourcesVisitor<'_> {
    type Value = u64;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map of source checkpoints")
    }

    fn visit_map<M>(self, mut map: M) -> Result<Self::Value, M::Error>
    where
        M: MapAccess<'de>,
    {
        let mut count = 0_u64;
        while let Some((key, checkpoint)) = map.next_entry::<String, StoredCheckpoint>()? {
            database::upsert(self.connection, &SourceKey::new(key), &checkpoint)
                .map_err(serde::de::Error::custom)?;
            count = count
                .checked_add(1)
                .ok_or_else(|| serde::de::Error::custom("too many source checkpoints"))?;
        }
        Ok(count)
    }
}

fn rename_legacy(path: &Path) -> Result<(), String> {
    let mut target = PathBuf::from(format!("{}.migrated", path.to_string_lossy()));
    let mut suffix = 1_u32;
    while target.exists() {
        target = PathBuf::from(format!("{}.migrated.{suffix}", path.to_string_lossy()));
        suffix = suffix.saturating_add(1);
    }
    fs::rename(path, &target).map_err(|error| error.to_string())?;
    key::sync_parent(path)
}
