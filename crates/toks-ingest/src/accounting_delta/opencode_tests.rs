use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use super::tests::{options, setup};

#[test]
fn opencode_database_is_collected_as_its_own_source() {
    fn write_opencode_db(home: &Path) -> PathBuf {
        let db_path = home.join(".local/share/opencode/opencode.db");
        fs::create_dir_all(db_path.parent().unwrap()).unwrap();
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(
            "CREATE TABLE message (
                id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                data TEXT NOT NULL
            );",
        )
        .unwrap();
        let data = r#"{
            "role": "assistant",
            "modelID": "claude-sonnet-4",
            "providerID": "anthropic",
            "tokens": { "input": 10, "output": 2, "reasoning": 0, "cache": { "read": 3, "write": 0 } },
            "time": { "created": 1700000000000.0 }
        }"#;
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params!["msg_001", "ses_001", data],
        )
        .unwrap();
        drop(conn);
        db_path
    }

    let (home, _state, mut collector) = setup();
    super::tests::write_initial(home.path());
    write_opencode_db(home.path());

    let delta = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(delta.sources.len(), 2);
    let opencode = delta
        .sources
        .iter()
        .find(|source| source.observations.iter().any(|o| o.client == "opencode"))
        .expect("an OpenCode source is collected");
    let observation = opencode
        .observations
        .iter()
        .find(|observation| observation.client == "opencode")
        .unwrap();
    assert_eq!(observation.model_id, "claude-sonnet-4");
    assert_eq!(observation.tokens.input, 10);
    assert!(opencode.backfill_complete);

    collector.commit(&delta).unwrap();
    let unchanged = collector.collect(options(home.path()), None).unwrap();
    assert!(unchanged.sources.is_empty());
}
