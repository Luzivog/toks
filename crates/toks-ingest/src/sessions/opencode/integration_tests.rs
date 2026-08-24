use super::*;
use std::path::PathBuf;

#[test]
#[ignore] // Run manually with: cargo test integration -- --ignored
fn test_parse_real_sqlite_db() {
    let home = std::env::var("HOME").unwrap();
    let db_path = PathBuf::from(format!("{}/.local/share/opencode/opencode.db", home));

    if !db_path.exists() {
        println!("Skipping: OpenCode database not found at {:?}", db_path);
        return;
    }

    let messages = parse_opencode_sqlite(&db_path);
    println!("Parsed {} messages from SQLite", messages.len());

    if !messages.is_empty() {
        let first = &messages[0];
        println!(
            "First message: model={}, provider={}, tokens={:?}",
            first.model_id, first.provider_id, first.tokens
        );
    }

    assert!(
        !messages.is_empty(),
        "Expected to parse some messages from SQLite"
    );
}
