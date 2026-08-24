use super::*;
fn create_hermes_sqlite_db(db_path: &std::path::Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute_batch(
            "CREATE TABLE sessions (
                id TEXT PRIMARY KEY,
                source TEXT NOT NULL,
                model TEXT,
                started_at REAL NOT NULL,
                message_count INTEGER DEFAULT 0,
                input_tokens INTEGER DEFAULT 0,
                output_tokens INTEGER DEFAULT 0,
                cache_read_tokens INTEGER DEFAULT 0,
                cache_write_tokens INTEGER DEFAULT 0,
                reasoning_tokens INTEGER DEFAULT 0,
                billing_provider TEXT,
                estimated_cost_usd REAL,
                actual_cost_usd REAL
            );
            CREATE TABLE session_model_usage (
                session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
                model TEXT NOT NULL,
                billing_provider TEXT NOT NULL DEFAULT '',
                billing_base_url TEXT NOT NULL DEFAULT '',
                billing_mode TEXT NOT NULL DEFAULT '',
                task TEXT NOT NULL DEFAULT '',
                api_call_count INTEGER NOT NULL DEFAULT 0,
                input_tokens INTEGER NOT NULL DEFAULT 0,
                output_tokens INTEGER NOT NULL DEFAULT 0,
                cache_read_tokens INTEGER NOT NULL DEFAULT 0,
                cache_write_tokens INTEGER NOT NULL DEFAULT 0,
                reasoning_tokens INTEGER NOT NULL DEFAULT 0,
                estimated_cost_usd REAL NOT NULL DEFAULT 0,
                actual_cost_usd REAL NOT NULL DEFAULT 0,
                cost_status TEXT,
                cost_source TEXT,
                first_seen REAL,
                last_seen REAL,
                PRIMARY KEY (session_id, model, billing_provider, billing_base_url, billing_mode, task)
            );",
        )
        .unwrap();
    conn
}

fn create_zed_sqlite_db(db_path: &std::path::Path) -> rusqlite::Connection {
    let conn = rusqlite::Connection::open(db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE threads (
                id TEXT PRIMARY KEY,
                summary TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                data_type TEXT NOT NULL,
                data BLOB NOT NULL
            );",
    )
    .unwrap();
    conn
}

fn insert_zed_thread(conn: &rusqlite::Connection, id: &str, model: &str) {
    let payload = format!(
        r#"{{
                "version": "0.3.0",
                "title": "Test thread",
                "updated_at": "2026-05-01T12:30:00Z",
                "request_token_usage": {{
                    "turn-1": {{
                        "input_tokens": 42,
                        "output_tokens": 7,
                        "cache_creation_input_tokens": 3,
                        "cache_read_input_tokens": 5
                    }}
                }},
                "model": {{
                    "provider": "zed.dev",
                    "model": "{model}"
                }},
                "imported": false
            }}"#
    );
    conn.execute(
            "INSERT INTO threads (id, summary, updated_at, data_type, data) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params![id, "Test thread", "2026-05-01T12:30:00Z", "json", payload.as_bytes()],
        )
        .unwrap();
}

fn insert_hermes_session(
    conn: &rusqlite::Connection,
    id: &str,
    model: &str,
    message_count: i64,
    input_tokens: i64,
    output_tokens: i64,
    actual_cost_usd: f64,
) {
    conn.execute(
            "INSERT INTO sessions (
                id, source, model, started_at, message_count,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
                billing_provider, estimated_cost_usd, actual_cost_usd
            ) VALUES (?1, 'cli', ?2, 1775001102.0, ?3, ?4, ?5, 0, 0, 0, 'anthropic', NULL, ?6)",
            rusqlite::params![
                id,
                model,
                message_count,
                input_tokens,
                output_tokens,
                actual_cost_usd
            ],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO session_model_usage (
                session_id, model, billing_provider, billing_base_url, billing_mode, task,
                input_tokens, output_tokens, cache_read_tokens, cache_write_tokens, reasoning_tokens,
                estimated_cost_usd, actual_cost_usd
            ) VALUES (?1, ?2, 'anthropic', '', '', '', ?3, ?4, 0, 0, 0, 0, ?5)",
            rusqlite::params![
                id,
                model,
                input_tokens,
                output_tokens,
                actual_cost_usd
            ],
        )
        .unwrap();
}

#[test]
fn test_parse_local_clients_honors_scanner_extra_scan_paths_for_hermes_profile_db() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let profile_dir = temp_dir.path().join("external-hermes/director_planning");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    let conn = create_hermes_sqlite_db(&profile_db);
    insert_hermes_session(
        &conn,
        "hermes-extra-session",
        "claude-sonnet-4",
        2,
        100,
        25,
        0.07,
    );
    drop(conn);

    let parsed_default = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["hermes".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();
    assert_eq!(parsed_default.counts.get(ClientId::Hermes), 0);
    assert!(parsed_default.messages.is_empty());

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert("hermes".to_string(), vec![profile_dir]);
    let parsed_with_settings = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["hermes".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    assert_eq!(parsed_with_settings.counts.get(ClientId::Hermes), 2);
    assert_eq!(parsed_with_settings.messages.len(), 1);
    assert_eq!(parsed_with_settings.messages[0].client, "hermes");
    assert_eq!(
        parsed_with_settings.messages[0].agent.as_deref(),
        Some("Hermes Agent")
    );
    assert_eq!(
        parsed_with_settings.messages[0].session_id,
        "hermes-extra-session"
    );
    assert_eq!(parsed_with_settings.messages[0].model_id, "claude-sonnet-4");
    assert_eq!(parsed_with_settings.messages[0].input, 100);
    assert_eq!(parsed_with_settings.messages[0].output, 25);
}

#[test]
fn test_parse_local_clients_honors_scanner_extra_scan_paths_for_zed_threads_db() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let extra_threads_dir = temp_dir.path().join("custom-zed/threads");
    std::fs::create_dir_all(&extra_threads_dir).unwrap();
    let threads_db = extra_threads_dir.join("threads.db");
    let conn = create_zed_sqlite_db(&threads_db);
    insert_zed_thread(&conn, "zed-extra-thread", "claude-sonnet-4-5");
    drop(conn);

    let parsed_default = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["zed".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();
    assert_eq!(parsed_default.counts.get(ClientId::Zed), 0);
    assert!(parsed_default.messages.is_empty());

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert("zed".to_string(), vec![extra_threads_dir]);
    let parsed_with_settings = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["zed".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    assert_eq!(parsed_with_settings.counts.get(ClientId::Zed), 1);
    assert_eq!(parsed_with_settings.messages.len(), 1);
    assert_eq!(parsed_with_settings.messages[0].client, "zed");
    assert_eq!(
        parsed_with_settings.messages[0].session_id,
        "zed-extra-thread"
    );
    assert_eq!(
        parsed_with_settings.messages[0].model_id,
        "claude-sonnet-4-5"
    );
    assert_eq!(parsed_with_settings.messages[0].input, 42);
    assert_eq!(parsed_with_settings.messages[0].output, 7);
}

#[test]
fn test_parse_local_clients_dedups_zed_threads_across_default_and_extra_dbs() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Place threads.db at the default platform path so the scanner finds it
    // as `zed_db` AND we also pass it via extraScanPaths.
    let default_threads_dir = temp_dir.path().join(".local/share/zed/threads");
    std::fs::create_dir_all(&default_threads_dir).unwrap();
    let default_db = default_threads_dir.join("threads.db");
    let conn = create_zed_sqlite_db(&default_db);
    insert_zed_thread(&conn, "shared-zed-thread", "claude-sonnet-4-5");
    drop(conn);

    // Point extraScanPaths.zed at the same directory — dedup should prevent
    // the thread from appearing twice.
    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert("zed".to_string(), vec![default_threads_dir.clone()]);
    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["zed".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    // Should see exactly 1 message, not 2 (deduped by canonicalize).
    assert_eq!(parsed.counts.get(ClientId::Zed), 1);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].session_id, "shared-zed-thread");
}

#[test]
fn test_parse_local_clients_zed_extra_scan_paths_nonexistent_dir_is_silent() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert(
        "zed".to_string(),
        vec![temp_dir.path().join("does/not/exist")],
    );
    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["zed".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::Zed), 0);
    assert!(parsed.messages.is_empty());
}

#[test]
fn test_parse_local_clients_dedups_hermes_sessions_across_default_and_extra_dbs() {
    let temp_dir = tempfile::TempDir::new().unwrap();

    let default_dir = temp_dir.path().join(".hermes");
    std::fs::create_dir_all(&default_dir).unwrap();
    let default_db = default_dir.join("state.db");
    let default_conn = create_hermes_sqlite_db(&default_db);
    insert_hermes_session(
        &default_conn,
        "shared-hermes-session",
        "claude-sonnet-4",
        2,
        100,
        25,
        0.07,
    );
    drop(default_conn);

    let profile_dir = temp_dir.path().join(".hermes/profiles/director_planning");
    std::fs::create_dir_all(&profile_dir).unwrap();
    let profile_db = profile_dir.join("state.db");
    let profile_conn = create_hermes_sqlite_db(&profile_db);
    insert_hermes_session(
        &profile_conn,
        "shared-hermes-session",
        "claude-sonnet-4",
        9,
        999,
        999,
        9.99,
    );
    drop(profile_conn);

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert("hermes".to_string(), vec![profile_db]);
    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["hermes".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::Hermes), 2);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].session_id, "shared-hermes-session");
    assert_eq!(parsed.messages[0].input, 100);
    assert_eq!(parsed.messages[0].output, 25);
}
