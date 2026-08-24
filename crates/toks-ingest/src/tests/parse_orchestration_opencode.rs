use super::support::*;
#[allow(clippy::too_many_arguments)]
pub(super) fn build_opencode_sqlite_payload(
    created_ms: f64,
    completed_ms: f64,
    input: i64,
    output: i64,
    reasoning: i64,
    cache_read: i64,
    cache_write: i64,
    cost: f64,
) -> String {
    format!(
        r#"{{
                "role": "assistant",
                "modelID": "claude-sonnet-4",
                "providerID": "anthropic",
                "cost": {cost},
                "tokens": {{
                    "input": {input},
                    "output": {output},
                    "reasoning": {reasoning},
                    "cache": {{ "read": {cache_read}, "write": {cache_write} }}
                }},
                "time": {{ "created": {created_ms}, "completed": {completed_ms} }},
                "mode": "build"
            }}"#
    )
}

pub(super) fn create_opencode_sqlite_db(db_path: &std::path::Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(db_path).unwrap();
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

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_includes_opencodereview() {
    // Regression: opencodereview declares submit_default, and
    // parse_local_clients has always parsed it, so `tokscope report`
    // showed the usage. But the submit path
    // (parse_all_messages_with_pricing_with_env_strategy) had no
    // opencodereview block at all, so none of that usage was ever
    // uploaded. Pin the submit path specifically — a green
    // parse_local_clients test cannot catch this.
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home
            .path()
            .join(".opencodereview/sessions/-home-user-project");
        std::fs::create_dir_all(&session_dir).unwrap();
        std::fs::write(
                session_dir.join("session-1.jsonl"),
                r#"{"type":"session_start","sessionId":"session-1","timestamp":"2026-01-15T10:00:00Z","cwd":"/home/user/project","model":"claude-sonnet-4-20250514"}
{"type":"llm_response","sessionId":"session-1","timestamp":"2026-01-15T10:00:05Z","model":"claude-sonnet-4-20250514","duration_ms":1500,"usage":{"prompt_tokens":1000,"completion_tokens":200,"cache_read_tokens":500,"cache_write_tokens":100}}
{"type":"llm_response","sessionId":"session-1","timestamp":"2026-01-15T10:01:00Z","model":"gpt-4o","duration_ms":900,"usage":{"prompt_tokens":300,"completion_tokens":50,"cache_read_tokens":0,"cache_write_tokens":0}}"#,
            )
            .unwrap();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencodereview".to_string()],
            None,
        );

        assert_eq!(messages.len(), 2);
        assert!(messages.iter().all(|m| m.client == "opencodereview"));
        assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 1300);
        assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 250);
        assert_eq!(
            messages.iter().map(|m| m.tokens.cache_read).sum::<i64>(),
            500
        );
        assert_eq!(
            messages.iter().map(|m| m.tokens.cache_write).sum::<i64>(),
            100
        );
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_dedups_across_channel_suffixed_opencode_dbs() {
    // Regression guard: a session that appears in both `opencode.db` and
    // `opencode-<channel>.db` (e.g. the user switches channels mid-session)
    // must only be counted once.
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let db_dir = source_home.path().join(".local/share/opencode");
        std::fs::create_dir_all(&db_dir).unwrap();

        let schema = "PRAGMA journal_mode=WAL;
                 PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE message (
                     id TEXT PRIMARY KEY,
                     session_id TEXT NOT NULL,
                     data TEXT NOT NULL
                 );";
        let row = |input: u64, ts: u64| {
            format!(
                r#"{{
                        "role": "assistant",
                        "modelID": "claude-sonnet-4",
                        "providerID": "anthropic",
                        "tokens": {{ "input": {input}, "output": 10, "reasoning": 0, "cache": {{ "read": 0, "write": 0 }} }},
                        "time": {{ "created": {ts}.0 }}
                    }}"#
            )
        };

        let default_db = db_dir.join("opencode.db");
        let conn = rusqlite::Connection::open(&default_db).unwrap();
        conn.execute_batch(schema).unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "shared-msg",
                "session-shared",
                row(100, 1_700_000_000_000u64)
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "latest-only",
                "session-latest",
                row(200, 1_700_000_001_000u64)
            ],
        )
        .unwrap();
        drop(conn);

        let stable_db = db_dir.join("opencode-stable.db");
        let conn = rusqlite::Connection::open(&stable_db).unwrap();
        conn.execute_batch(schema).unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "shared-msg",
                "session-shared",
                row(100, 1_700_000_000_000u64)
            ],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "stable-only",
                "session-stable",
                row(300, 1_700_000_002_000u64)
            ],
        )
        .unwrap();
        drop(conn);

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert_eq!(
            messages.len(),
            3,
            "expected 3 unique messages (shared + latest-only + stable-only), got {}",
            messages.len()
        );
        let mut ids: Vec<String> = messages
            .iter()
            .filter_map(|m| m.dedup_key.clone())
            .collect();
        ids.sort();
        assert_eq!(ids, vec!["latest-only", "shared-msg", "stable-only"]);

        let messages_warm = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert_eq!(
            messages_warm.len(),
            3,
            "warm cache must also dedup shared message across channel dbs"
        );
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_opencode_sqlite_deduplicates_forked_history() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let db_dir = source_home.path().join(".local/share/opencode");
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("opencode.db");
        let conn = create_opencode_sqlite_db(&db_path);

        let msg_a = build_opencode_sqlite_payload(
            1_700_000_000_000.0,
            1_700_000_000_500.0,
            100,
            50,
            0,
            10,
            5,
            0.01,
        );
        let msg_b = build_opencode_sqlite_payload(
            1_700_000_001_000.0,
            1_700_000_001_500.0,
            200,
            80,
            10,
            20,
            0,
            0.02,
        );
        let msg_c = build_opencode_sqlite_payload(
            1_700_000_002_000.0,
            1_700_000_002_500.0,
            300,
            120,
            15,
            0,
            0,
            0.03,
        );

        for (id, session_id, payload) in [
            ("root_a", "root", msg_a.as_str()),
            ("root_b", "root", msg_b.as_str()),
            ("fork_a_copy", "fork", msg_a.as_str()),
            ("fork_b_copy", "fork", msg_b.as_str()),
            ("fork_c_new", "fork", msg_c.as_str()),
        ] {
            conn.execute(
                "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
                rusqlite::params![id, session_id, payload],
            )
            .unwrap();
        }
        drop(conn);

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );

        assert_eq!(messages.len(), 3);
        assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 600);
        assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 250);
        assert_eq!(messages.iter().map(|m| m.cost).sum::<f64>(), 0.06);
    }
}
