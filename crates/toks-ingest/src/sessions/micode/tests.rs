use super::*;
use rusqlite::Connection;

mod dedup_and_provenance;
mod message_identity;
mod metadata_and_defaults;

fn create_micode_sqlite_db(db_path: &Path) -> Connection {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            data TEXT NOT NULL
        );",
    )
    .unwrap();
    conn
}
