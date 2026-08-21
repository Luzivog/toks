use rusqlite::Connection;

pub(super) const VERSION: u32 = 1;

const SCHEMA: &str = r#"
BEGIN IMMEDIATE;
CREATE TABLE sources (
    source_key TEXT PRIMARY KEY NOT NULL,
    kind INTEGER NOT NULL,
    parser_version INTEGER NOT NULL,
    committed_offset INTEGER NOT NULL,
    source_size INTEGER NOT NULL,
    modified_ns INTEGER NOT NULL,
    content_hash BLOB NOT NULL,
    prefix_samples BLOB NOT NULL,
    codex_state BLOB
) STRICT;
CREATE TABLE meta (
    singleton INTEGER PRIMARY KEY CHECK (singleton = 1),
    rotation_cursor TEXT,
    legacy_v1_imported INTEGER NOT NULL DEFAULT 0 CHECK (legacy_v1_imported IN (0, 1))
) STRICT;
INSERT INTO meta(singleton) VALUES (1);
PRAGMA user_version = 1;
COMMIT;
"#;

pub(super) fn configure(connection: &Connection) -> Result<(), String> {
    connection
        .execute_batch(
            "PRAGMA journal_mode = DELETE;
             PRAGMA synchronous = FULL;
             PRAGMA temp_store = MEMORY;
             PRAGMA foreign_keys = ON;",
        )
        .map_err(|error| error.to_string())
}

pub(super) fn initialize(connection: &Connection) -> Result<(), String> {
    let version = version(connection)?;
    if version > VERSION {
        return Err("unsupported accounting checkpoint schema".to_string());
    }
    if version == VERSION {
        return validate(connection);
    }
    let tables: u32 = connection
        .query_row(
            "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name NOT LIKE 'sqlite_%'",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    if tables != 0 {
        return Err("unsupported accounting checkpoint schema".to_string());
    }
    connection
        .execute_batch(SCHEMA)
        .map_err(|error| error.to_string())?;
    validate(connection)
}

pub(super) fn validate(connection: &Connection) -> Result<(), String> {
    if version(connection)? != VERSION {
        return Err("unsupported accounting checkpoint schema".to_string());
    }
    let integrity: String = connection
        .query_row("PRAGMA quick_check", [], |row| row.get(0))
        .map_err(|error| error.to_string())?;
    if integrity != "ok" {
        return Err("accounting checkpoint database failed integrity check".to_string());
    }
    connection
        .query_row(
            "SELECT rotation_cursor, legacy_v1_imported FROM meta WHERE singleton = 1",
            [],
            |_| Ok(()),
        )
        .map_err(|error| error.to_string())
}

fn version(connection: &Connection) -> Result<u32, String> {
    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}
