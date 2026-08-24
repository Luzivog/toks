use super::*;
#[test]
fn test_parse_local_clients_honors_devin_cli_extra_scan_paths() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let external_dir = temp_dir.path().join("imports/devin/profile");
    std::fs::create_dir_all(&external_dir).unwrap();
    let external_db = external_dir.join("sessions.db");
    let conn = rusqlite::Connection::open(&external_db).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 working_directory TEXT NOT NULL,
                 backend_type TEXT NOT NULL,
                 model TEXT NOT NULL,
                 title TEXT,
                 agent_mode TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 last_activity_at INTEGER NOT NULL
             );
             CREATE TABLE message_nodes (
                 row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 node_id INTEGER NOT NULL,
                 parent_node_id INTEGER,
                 chat_message TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 metadata TEXT
             );",
    )
    .unwrap();
    conn.execute(
            "INSERT INTO sessions (id, working_directory, backend_type, model, agent_mode, created_at, last_activity_at) VALUES ('external-session', '/tmp/project', 'windsurf', 'gpt-5', 'accept-edits', 1, 1)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO message_nodes (session_id, node_id, chat_message, created_at) VALUES ('external-session', 1, ?1, 1700000000)",
            [r#"{"role":"assistant","metadata":{"metrics":{"input_tokens":42,"output_tokens":7}}}"#],
        )
        .unwrap();
    drop(conn);

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert("devin-cli".to_string(), vec![external_dir]);
    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["devin-cli".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::DevinCli), 1);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "devin-cli");
    assert_eq!(parsed.messages[0].session_id, "external-session");
}

#[test]
fn test_parse_local_clients_devin_zero_cli_usage_does_not_suppress_desktop() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join(".local/share/devin/cli/sessions.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 working_directory TEXT NOT NULL,
                 backend_type TEXT NOT NULL,
                 model TEXT NOT NULL,
                 title TEXT,
                 agent_mode TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 last_activity_at INTEGER NOT NULL
             );
             CREATE TABLE message_nodes (
                 row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 node_id INTEGER NOT NULL,
                 parent_node_id INTEGER,
                 chat_message TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 metadata TEXT
             );",
    )
    .unwrap();
    conn.execute(
            "INSERT INTO sessions (id, working_directory, backend_type, model, title, agent_mode, created_at, last_activity_at) VALUES ('cli-session', '/tmp/project', 'windsurf', 'gpt-5', 'Desktop task', 'accept-edits', 1, 1)",
            [],
        )
        .unwrap();
    conn.execute(
            "INSERT INTO message_nodes (session_id, node_id, chat_message, created_at) VALUES ('cli-session', 1, ?1, 1700000000)",
            [r#"{"role":"assistant","metadata":{"metrics":{"input_tokens":0,"output_tokens":0}}}"#],
        )
        .unwrap();
    drop(conn);

    let desktop_dir = temp_dir
        .path()
        .join("Library/Application Support/Devin/User/acp-events");
    std::fs::create_dir_all(&desktop_dir).unwrap();
    std::fs::write(
            desktop_dir.join("desktop-file.ndjson"),
            concat!(
                r#"{"notification":{"sessionUpdate":"session_info_update","title":"Desktop task"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":100,"cognition.ai/outputTokens":20,"cognition.ai/cachedReadTokens":10}}}"#,
                "\n"
            ),
        )
        .unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["devin-cli".to_string(), "devin-desktop".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::DevinCli), 0);
    assert_eq!(parsed.counts.get(ClientId::DevinDesktop), 1);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "devin-desktop");
    assert_eq!(parsed.messages[0].session_id, "cli-session");
    assert_eq!(parsed.messages[0].model_id, "gpt-5");
    assert_eq!(parsed.messages[0].input, 90);
    assert_eq!(parsed.messages[0].cache_read, 10);
}

#[test]
fn test_parse_local_clients_devin_nonzero_cli_usage_dedups_desktop_row_but_keeps_raw_count() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join(".local/share/devin/cli/sessions.db");
    std::fs::create_dir_all(db_path.parent().unwrap()).unwrap();
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 working_directory TEXT NOT NULL,
                 backend_type TEXT NOT NULL,
                 model TEXT NOT NULL,
                 title TEXT,
                 agent_mode TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 last_activity_at INTEGER NOT NULL
             );
             CREATE TABLE message_nodes (
                 row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                 session_id TEXT NOT NULL,
                 node_id INTEGER NOT NULL,
                 parent_node_id INTEGER,
                 chat_message TEXT NOT NULL,
                 created_at INTEGER NOT NULL,
                 metadata TEXT
             );",
    )
    .unwrap();
    conn.execute(
            "INSERT INTO sessions (id, working_directory, backend_type, model, title, agent_mode, created_at, last_activity_at) VALUES ('cli-session', '/tmp/project', 'windsurf', 'gpt-5', 'Desktop task', 'accept-edits', 1, 1)",
            [],
        )
        .unwrap();
    // Unlike the zero-usage regression test above, this CLI row carries
    // real attributable usage, so it must NOT be filtered by
    // `parse_devin_cli_sqlite`'s zero-metric guard. That means its
    // session id lands in `cli_session_ids`, which is exactly the
    // condition needed to exercise the dedup filter against the
    // matching Desktop NDJSON session.
    conn.execute(
            "INSERT INTO message_nodes (session_id, node_id, chat_message, created_at) VALUES ('cli-session', 1, ?1, 1700000000)",
            [r#"{"role":"assistant","metadata":{"metrics":{"input_tokens":50,"output_tokens":25}}}"#],
        )
        .unwrap();
    drop(conn);

    let desktop_dir = temp_dir
        .path()
        .join("Library/Application Support/Devin/User/acp-events");
    std::fs::create_dir_all(&desktop_dir).unwrap();
    std::fs::write(
            desktop_dir.join("desktop-file.ndjson"),
            concat!(
                r#"{"notification":{"sessionUpdate":"session_info_update","title":"Desktop task"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":100,"cognition.ai/outputTokens":20,"cognition.ai/cachedReadTokens":10}}}"#,
                "\n"
            ),
        )
        .unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["devin-cli".to_string(), "devin-desktop".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    // The Desktop row shares its resolved session id with the CLI row,
    // so it must be deduped out of `messages` and attributed to
    // devin-cli instead.
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "devin-cli");
    assert_eq!(parsed.messages[0].session_id, "cli-session");

    // But the `clients` command count must still reflect the raw,
    // pre-dedup Desktop discovery so Desktop usage doesn't appear to
    // vanish when it overlaps with a CLI session.
    assert_eq!(parsed.counts.get(ClientId::DevinCli), 1);
    assert!(parsed.counts.get(ClientId::DevinDesktop) > 0);
}

#[test]
fn test_parse_local_clients_desktop_uses_configured_cli_lookup_without_cli_usage() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let external_dir = temp_dir.path().join("imports/devin/profile");
    std::fs::create_dir_all(&external_dir).unwrap();
    let external_db = external_dir.join("sessions.db");
    let conn = rusqlite::Connection::open(&external_db).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 title TEXT,
                 model TEXT,
                 working_directory TEXT
             );",
    )
    .unwrap();
    conn.execute(
            "INSERT INTO sessions (id, title, model, working_directory) VALUES ('external-session', 'External desktop task', 'claude-sonnet-4', '/tmp/external-project')",
            [],
        )
        .unwrap();
    drop(conn);

    let desktop_dir = temp_dir
        .path()
        .join("Library/Application Support/Devin/User/acp-events");
    std::fs::create_dir_all(&desktop_dir).unwrap();
    std::fs::write(
            desktop_dir.join("desktop-file.ndjson"),
            concat!(
                r#"{"notification":{"sessionUpdate":"session_info_update","title":"External desktop task"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":100,"cognition.ai/outputTokens":20}}}"#,
                "\n"
            ),
        )
        .unwrap();

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert("devin-cli".to_string(), vec![external_dir]);
    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["devin-desktop".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings {
            extra_scan_paths,
            ..Default::default()
        },
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::DevinCli), 0);
    assert_eq!(parsed.counts.get(ClientId::DevinDesktop), 1);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "devin-desktop");
    assert_eq!(parsed.messages[0].session_id, "external-session");
    assert_eq!(parsed.messages[0].model_id, "claude-sonnet-4");
    assert_eq!(
        parsed.messages[0].workspace_key.as_deref(),
        Some("/tmp/external-project")
    );
}

#[test]
fn test_devin_desktop_lookup_cache_separates_database_snapshots() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let db_path = temp_dir.path().join("sessions.db");
    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute_batch(
        "CREATE TABLE sessions (
                 id TEXT PRIMARY KEY,
                 title TEXT,
                 model TEXT,
                 working_directory TEXT
             );
             INSERT INTO sessions (id, title, model, working_directory)
             VALUES ('cli-session', 'Snapshot task', 'gpt-5', '/tmp/project');",
    )
    .unwrap();
    drop(conn);

    let desktop_path = temp_dir.path().join("desktop-file.ndjson");
    std::fs::write(
            &desktop_path,
            concat!(
                r#"{"notification":{"sessionUpdate":"session_info_update","title":"Snapshot task"}}"#,
                "\n",
                r#"{"notification":{"sessionUpdate":"usage_update","_meta":{"cognition.ai/inputTokens":100,"cognition.ai/outputTokens":20}}}"#,
                "\n"
            ),
        )
        .unwrap();

    let first_fingerprint =
        match message_cache::SourceFingerprint::check_devin_desktop_path_samples_only(
            &desktop_path,
            std::slice::from_ref(&db_path),
            None,
        )
        .unwrap()
        {
            message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
            message_cache::FingerprintStatus::Unchanged => {
                panic!("an uncached Desktop source must build a fingerprint")
            }
        };
    let lookup_cache = std::sync::Mutex::new(HashMap::new());
    let first_cell = crate::devin_desktop_lookup_cell_for_snapshot(
        &lookup_cache,
        std::slice::from_ref(&db_path),
        &first_fingerprint,
    );
    let first_lookup = first_cell.get_or_init(|| {
        crate::sessions::devin::load_devin_desktop_session_lookup(std::slice::from_ref(&db_path))
    });
    let first_messages =
        crate::sessions::devin::parse_devin_desktop_ndjson_with_lookup(&desktop_path, first_lookup);
    assert_eq!(first_messages[0].model_id, "gpt-5");

    let conn = rusqlite::Connection::open(&db_path).unwrap();
    conn.execute(
        "UPDATE sessions SET model = 'claude-sonnet-4' WHERE id = 'cli-session'",
        [],
    )
    .unwrap();
    drop(conn);

    let second_fingerprint =
        match message_cache::SourceFingerprint::check_devin_desktop_path_samples_only(
            &desktop_path,
            std::slice::from_ref(&db_path),
            None,
        )
        .unwrap()
        {
            message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
            message_cache::FingerprintStatus::Unchanged => {
                panic!("an uncached Desktop source must build a fingerprint")
            }
        };
    assert_ne!(
        first_fingerprint.related_files,
        second_fingerprint.related_files
    );

    let second_cell = crate::devin_desktop_lookup_cell_for_snapshot(
        &lookup_cache,
        std::slice::from_ref(&db_path),
        &second_fingerprint,
    );
    assert!(
        !Arc::ptr_eq(&first_cell, &second_cell),
        "different database snapshots must not share a lookup cell"
    );
    let second_lookup = second_cell.get_or_init(|| {
        crate::sessions::devin::load_devin_desktop_session_lookup(std::slice::from_ref(&db_path))
    });
    let second_messages = crate::sessions::devin::parse_devin_desktop_ndjson_with_lookup(
        &desktop_path,
        second_lookup,
    );
    assert_eq!(second_messages[0].model_id, "claude-sonnet-4");
    assert_eq!(lookup_cache.lock().unwrap().len(), 2);
}
