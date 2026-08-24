use super::parse_orchestration_opencode::{
    build_opencode_sqlite_payload, create_opencode_sqlite_db,
};
use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_parse_local_clients_opencode_sqlite_counts_deduplicated_forked_history() {
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

        let parsed = parse_local_clients(LocalParseOptions {
            home_dir: Some(source_home.path().to_str().unwrap().to_string()),
            use_env_roots: false,
            clients: Some(vec!["opencode".to_string()]),
            since: None,
            until: None,
            year: None,
            scanner_settings: scanner::ScannerSettings::default(),
        })
        .unwrap();

        assert_eq!(parsed.counts.get(ClientId::OpenCode), 3);
        assert_eq!(parsed.messages.len(), 3);
        assert_eq!(parsed.messages.iter().map(|m| m.input).sum::<i64>(), 600);
        assert_eq!(parsed.messages.iter().map(|m| m.output).sum::<i64>(), 250);
    }
}

#[test]
fn test_parse_local_clients_honors_scanner_settings_opencode_db_paths() {
    // Regression guard: `parse_local_clients` used to call
    // `scan_all_clients_with_env_strategy`, which silently dropped
    // `options.scanner_settings`. Users with
    // `scanner.opencodeDbPaths` pointing at an OPENCODE_DB outside the
    // XDG data dir would see no rows through the clients/wrapped
    // command paths even though model/monthly/graph reports honored
    // the same config.
    let temp_dir = tempfile::TempDir::new().unwrap();
    // Deliberately do not create ~/.local/share/opencode so nothing
    // is auto-discoverable; the only db the scanner can find must
    // come from `scanner_settings`.
    let outside_dir = temp_dir.path().join("elsewhere");
    std::fs::create_dir_all(&outside_dir).unwrap();
    let external_db = outside_dir.join("opencode.db");

    let conn = rusqlite::Connection::open(&external_db).unwrap();
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
             CREATE TABLE message (
                 id TEXT PRIMARY KEY,
                 session_id TEXT NOT NULL,
                 data TEXT NOT NULL
             );",
    )
    .unwrap();
    conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params![
                "ext-msg-1",
                "ext-session",
                r#"{
                    "role": "assistant",
                    "modelID": "claude-sonnet-4",
                    "providerID": "anthropic",
                    "tokens": { "input": 42, "output": 7, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
                    "time": { "created": 1700000000000.0 }
                }"#
            ],
        )
        .unwrap();
    drop(conn);

    // Without scanner_settings: no rows (nothing auto-discoverable).
    let parsed_default = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["opencode".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();
    assert_eq!(parsed_default.counts.get(ClientId::OpenCode), 0);
    assert!(parsed_default.messages.is_empty());

    // With scanner_settings pointing at the external db: the user
    // row must show up.
    let parsed_with_settings = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["opencode".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            opencode_db_paths: vec![external_db.clone()],
            ..Default::default()
        },
    })
    .unwrap();
    assert_eq!(
        parsed_with_settings.counts.get(ClientId::OpenCode),
        1,
        "scanner.opencodeDbPaths must reach the parse_local_clients path"
    );
    assert_eq!(parsed_with_settings.messages.len(), 1);
    assert_eq!(parsed_with_settings.messages[0].client, "opencode");
    assert_eq!(parsed_with_settings.messages[0].model_id, "claude-sonnet-4");
}
