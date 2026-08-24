use super::*;
use rusqlite::{params, Connection};
use std::fs::{self, File};
use std::io::Write;

mod database_and_events;
mod shutdown_attribution;
mod shutdown_conservation;
mod shutdown_snapshots;

fn create_copilot_desktop_db(path: &Path) -> Connection {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE sessions (
            id TEXT,
            title TEXT,
            session_type TEXT,
            mode TEXT,
            model TEXT,
            total_input_tokens INTEGER,
            total_output_tokens INTEGER,
            total_cached_tokens INTEGER,
            total_reasoning_tokens INTEGER,
            total_nano_aiu INTEGER,
            created_at TEXT,
            agent TEXT,
            provider_id TEXT
        );
        "#,
    )
    .unwrap();
    conn
}

fn insert_session(
    conn: &Connection,
    id: &str,
    model: &str,
    input: i64,
    output: i64,
    cached: i64,
    reasoning: i64,
) {
    conn.execute(
        r#"
        INSERT INTO sessions (
            id, title, session_type, mode, model,
            total_input_tokens, total_output_tokens, total_cached_tokens,
            total_reasoning_tokens, total_nano_aiu, created_at, agent, provider_id
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        "#,
        params![
            id,
            "Test session",
            "chat",
            "agent",
            model,
            input,
            output,
            cached,
            reasoning,
            0_i64,
            "2026-07-01T12:34:56Z",
            "github.copilot.default",
            "github-copilot"
        ],
    )
    .unwrap();
}

/// Every real `events.jsonl` opens with `session.start`: the SDK refuses to
/// load a session whose first event is anything else, so a log that does
/// not start with one has lost its head. Fixtures that exercise shutdown
/// attribution open with it for the same reason real logs do.
const SESSION_START: &str = r#"{"type":"session.start","data":{},"id":"3f0a1c22-6b41-4d0e-9c7a-5e2b8d4f1a00","timestamp":"2026-07-01T19:00:00.000Z"}"#;

fn write_events(root: &Path, session_id: &str, lines: &[&str]) {
    let events_dir = root.join("session-state").join(session_id);
    fs::create_dir_all(&events_dir).unwrap();
    let mut file = File::create(events_dir.join("events.jsonl")).unwrap();
    for line in lines {
        writeln!(file, "{}", line).unwrap();
    }
}
