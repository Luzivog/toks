use rusqlite::Connection;

pub(super) fn source_count(connection: &Connection) -> Result<u64, String> {
    connection
        .query_row("SELECT count(*) FROM sources", [], |row| row.get(0))
        .map_err(|error| error.to_string())
}

pub(super) fn legacy_imported(connection: &Connection) -> Result<bool, String> {
    connection
        .query_row(
            "SELECT legacy_v1_imported FROM meta WHERE singleton = 1",
            [],
            |row| row.get::<_, i64>(0).map(|value| value == 1),
        )
        .map_err(|error| error.to_string())
}

pub(super) fn mark_legacy_imported(connection: &Connection) -> Result<(), String> {
    connection
        .execute(
            "UPDATE meta SET legacy_v1_imported = 1 WHERE singleton = 1",
            [],
        )
        .map(|_| ())
        .map_err(|error| error.to_string())
}

pub(super) fn load_rotation_cursor(connection: &Connection) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT rotation_cursor FROM meta WHERE singleton = 1",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())
}
