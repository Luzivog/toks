use super::*;
use rusqlite::Connection;

fn create_opencode_sqlite_db(db_path: &Path) -> Connection {
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

/// Build a database shaped like OpenCode v2 (`opencode-next.db`): an empty
/// `message` table plus the `session_message` + `session` tables that hold
/// the real per-message data. Mirrors the columns Toks actually reads.
fn create_opencode_v2_sqlite_db(db_path: &Path) -> Connection {
    let conn = Connection::open(db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            data TEXT NOT NULL
        );
        CREATE TABLE session (
            id TEXT PRIMARY KEY,
            directory TEXT NOT NULL,
            title TEXT
        );
        CREATE TABLE session_message (
            id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            type TEXT NOT NULL,
            data TEXT NOT NULL
        );",
    )
    .unwrap();
    conn
}

/// A representative v2 assistant payload: no `role` field, model + provider
/// nested under `$.model`, integer timestamps.
const V2_ASSISTANT_DATA: &str = r#"{
    "time": { "created": 1783882279705, "completed": 1783882279943 },
    "agent": "build",
    "model": { "id": "claude-sonnet-4", "providerID": "anthropic", "variant": "default" },
    "content": [],
    "finish": "stop",
    "cost": 0.0123,
    "tokens": {
        "input": 5519,
        "output": 20,
        "reasoning": 23,
        "cache": { "read": 100, "write": 50 }
    }
}"#;

mod legacy_json;
mod migration_cache;
mod sqlite_dedup;
mod sqlite_metadata;
mod v2;
