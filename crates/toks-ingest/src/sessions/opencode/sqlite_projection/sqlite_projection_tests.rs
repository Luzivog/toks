use rusqlite::Connection;

use super::COMPACT_MESSAGE_SQL;
use crate::sessions::opencode::OpenCodeMessage;

#[test]
fn projection_excludes_ignored_message_content() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute("CREATE TABLE message (data TEXT NOT NULL)", [])
        .unwrap();
    let ignored_content = "x".repeat(256 * 1024);
    let data = serde_json::json!({
        "role": "assistant",
        "modelID": "claude-sonnet-4",
        "providerID": "anthropic",
        "tokens": {
            "input": 10,
            "output": 2,
            "reasoning": 0,
            "cache": { "read": 3, "write": 0 }
        },
        "time": { "created": 1_700_000_000_000.0 },
        "content": ignored_content,
    })
    .to_string();
    conn.execute("INSERT INTO message (data) VALUES (?1)", [&data])
        .unwrap();

    let query = format!("SELECT {COMPACT_MESSAGE_SQL} FROM message");
    let compact: String = conn.query_row(&query, [], |row| row.get(0)).unwrap();
    assert!(
        compact.len() < 1024,
        "projection was {} bytes",
        compact.len()
    );
    assert!(!compact.contains(&ignored_content));

    let mut bytes = compact.into_bytes();
    let parsed: OpenCodeMessage = simd_json::from_slice(&mut bytes).unwrap();
    assert_eq!(
        parsed.resolve_model_id().as_deref(),
        Some("claude-sonnet-4")
    );
    assert_eq!(parsed.tokens.unwrap().input, 10);
}
