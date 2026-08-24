use super::parse_orchestration_misc_clients::write_kimi_repeated_status_fixture;
use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_parse_local_clients_kimi_deduplicates_repeated_status_updates() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_kimi_repeated_status_fixture(source_home.path());

        let parsed = parse_local_clients(LocalParseOptions {
            home_dir: Some(source_home.path().to_str().unwrap().to_string()),
            use_env_roots: false,
            clients: Some(vec!["kimi".to_string()]),
            since: None,
            until: None,
            year: None,
            scanner_settings: scanner::ScannerSettings::default(),
        })
        .unwrap();

        assert_eq!(parsed.counts.get(ClientId::Kimi), 4);
        assert_eq!(parsed.messages.len(), 4);
        assert_eq!(parsed.messages.iter().map(|m| m.input).sum::<i64>(), 40);
        assert_eq!(parsed.messages.iter().map(|m| m.output).sum::<i64>(), 5);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_local_clients_codebuff_freebuff_filters_stay_isolated() {
    // Freebuff and Codebuff share the manicode scan bucket (parser
    // partition the same file set). A single-client filter must not pick
    // up the other product's rows: codebuff-only must produce clean code
    // rows/zero freebuff count, and vice versa.
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    let manicode = source_home.path().join(".config").join("manicode");
    // An authoritative Codebuff chat: assistant message carries usage.
    let codebuff_chat = manicode
        .join("projects")
        .join("proj")
        .join("chats")
        .join("2026-08-07T05-21-00.000Z");
    std::fs::create_dir_all(&codebuff_chat).unwrap();
    std::fs::write(
        codebuff_chat.join("chat-messages.json"),
        r#"[
                { "variant": "user", "content": "hi", "timestamp": "2026-08-07T05:21:00.000Z" },
                { "variant": "ai", "timestamp": "2026-08-07T05:22:00.000Z",
                  "metadata": { "model": "claude-sonnet-4-20250514",
                                "usage": { "inputTokens": 500, "outputTokens": 200 } } }
            ]"#,
    )
    .unwrap();
    // A Freebuff chat: marked by its `base2-free*` root agent id, with no
    // authoritative usage — only estimated text.
    let freebuff_chat = manicode
        .join("projects")
        .join("proj")
        .join("chats")
        .join("2026-08-07T13-00-00.000Z");
    std::fs::create_dir_all(&freebuff_chat).unwrap();
    std::fs::write(
            freebuff_chat.join("chat-messages.json"),
            r#"[
                { "variant": "user", "content": "hello world", "timestamp": "2026-08-07T13:00:00.000Z" },
                { "variant": "ai", "timestamp": "2026-08-07T13:01:00.000Z", "blocks": [ { "content": "Hello!" } ],
                  "metadata": { "runState": { "sessionState": { "mainAgentState": {
                      "agentType": "base2-free-deepseek-flash" } } } } }
            ]"#,
        )
        .unwrap();

    let options_for = |clients: Vec<String>| LocalParseOptions {
        home_dir: Some(source_home.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(clients),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    };

    // codebuff-only: authoritative Codebuff row, zero estimated Freebuff rows.
    let codebuff_only = parse_local_clients(options_for(vec!["codebuff".to_string()])).unwrap();
    assert_eq!(codebuff_only.counts.get(ClientId::Codebuff), 1);
    assert_eq!(codebuff_only.counts.get(ClientId::Freebuff), 0);
    assert!(
        codebuff_only
            .messages
            .iter()
            .all(|m| m.client == "codebuff"),
        "all reported rows must be codebuff, got {:?}",
        codebuff_only
            .messages
            .iter()
            .map(|m| &m.client)
            .collect::<Vec<_>>()
    );

    // freebuff-only → estimated Freebuff rows, zero Codebuff rows.
    let free_only = parse_local_clients(options_for(vec!["freebuff".to_string()])).unwrap();
    assert_eq!(free_only.counts.get(ClientId::Freebuff), 1);
    assert_eq!(free_only.counts.get(ClientId::Codebuff), 0);
    assert!(
        free_only.messages.iter().all(|m| m.client == "freebuff"),
        "all reported rows must be freebuff, got {:?}",
        free_only
            .messages
            .iter()
            .map(|m| &m.client)
            .collect::<Vec<_>>()
    );
}

#[test]
fn test_parse_local_clients_preserves_gateway_message_client_counts() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let message_dir = temp_dir
        .path()
        .join(".local/share/opencode/storage/message/project-1");
    std::fs::create_dir_all(&message_dir).unwrap();
    std::fs::write(
            message_dir.join("msg_001.json"),
            r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"accounts/fireworks/models/deepseek-v3-0324","providerID":"fireworks","cost":0,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
        )
        .unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["opencode".to_string(), "synthetic".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::OpenCode), 1);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "opencode");
    assert_eq!(parsed.messages[0].model_id, "deepseek-v3-0324");
    // opencode now canonicalizes the provider segment like every other
    // session parser, so the raw "fireworks" gateway id resolves to its
    // canonical "fireworks_ai" tag.
    assert_eq!(parsed.messages[0].provider_id, "fireworks_ai");
}

#[test]
fn test_parse_local_clients_fireworks_provider_kept_under_synthetic_only_filter() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let message_dir = temp_dir
        .path()
        .join(".local/share/opencode/storage/message/project-1");
    std::fs::create_dir_all(&message_dir).unwrap();
    std::fs::write(
            message_dir.join("msg_001.json"),
            r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"accounts/fireworks/models/deepseek-v3-0324","providerID":"fireworks","cost":0.1,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
        )
        .unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["synthetic".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    assert_eq!(
        parsed.messages.len(),
        1,
        "fireworks gateway message must not be dropped when filtering for synthetic only"
    );
    assert_eq!(parsed.messages[0].client, "opencode");
    assert_eq!(parsed.messages[0].model_id, "deepseek-v3-0324");
    // Provider is canonicalized by the opencode parser (fireworks -> fireworks_ai).
    assert_eq!(parsed.messages[0].provider_id, "fireworks_ai");
}

#[test]
fn test_parse_local_clients_claude_filter_ignores_scanner_settings_opencode_db_paths() {
    // Regression guard for the scanner client-filter bypass: even
    // when `scanner.opencodeDbPaths` pins an external opencode db,
    // a `--clients claude` request must NOT pull in OpenCode rows.
    // Before the fix, the merge ran outside the OpenCode-enabled
    // guard so user-pinned dbs leaked through both `messages` and
    // `counts` (the latter is computed before the message-level
    // client filter, so even the post-filter pipeline could not
    // hide a leaked count).
    let temp_dir = tempfile::TempDir::new().unwrap();

    // Claude session: one assistant message, the only thing the
    // filter should accept.
    let claude_dir = temp_dir.path().join(".claude/projects/myproject");
    std::fs::create_dir_all(&claude_dir).unwrap();
    std::fs::write(
            claude_dir.join("conversation.jsonl"),
            r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#,
        )
        .unwrap();

    // External opencode.db that the user has pinned via
    // scanner.opencodeDbPaths. Without the fix, this would leak
    // into the Claude-only result.
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
                "leaked-opencode",
                "should-not-show-up",
                r#"{
                    "role": "assistant",
                    "modelID": "claude-sonnet-4",
                    "providerID": "anthropic",
                    "tokens": { "input": 9999, "output": 9999, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
                    "time": { "created": 1700000000000.0 }
                }"#
            ],
        )
        .unwrap();
    drop(conn);

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["claude".to_string()]),
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
        parsed.counts.get(ClientId::OpenCode),
        0,
        "OpenCode count must stay zero under a Claude-only filter even \
             when scanner.opencodeDbPaths is set"
    );
    assert_eq!(
        parsed.counts.get(ClientId::Claude),
        1,
        "Claude message must still be counted"
    );
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "claude");
    assert!(
        parsed.messages.iter().all(|m| m.client != "opencode"),
        "no OpenCode messages may leak into a Claude-only result, got {:?}",
        parsed.messages
    );
}

#[test]
fn test_parse_local_clients_claude_transcripts_count_only_usage_metadata() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let transcripts_dir = temp_dir.path().join(".claude/transcripts");
    std::fs::create_dir_all(&transcripts_dir).unwrap();
    std::fs::write(
            transcripts_dir.join("ses_123456789012345678901234567.jsonl"),
            r#"{"type":"user","timestamp":"2026-04-01T10:00:00.000Z","message":{"content":"Wrapped prompt"}}
{"type":"assistant","timestamp":"2026-04-01T10:00:01.000Z","requestId":"req_wrapper","message":{"id":"msg_wrapper","model":"claude-sonnet-4","usage":{"input_tokens":123,"output_tokens":45,"cache_read_input_tokens":67,"cache_creation_input_tokens":8}}}
"#,
        )
        .unwrap();
    std::fs::write(
            transcripts_dir.join("ses_765432109876543210987654321.jsonl"),
            r#"{"type":"user","timestamp":"2026-04-01T10:00:00.000Z","message":{"content":"Wrapped prompt"}}
{"type":"tool_use","timestamp":"2026-04-01T10:00:01.000Z","message":{"content":"Run tool"}}
{"type":"tool_result","timestamp":"2026-04-01T10:00:02.000Z","message":{"content":"Tool result"}}
"#,
        )
        .unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["claude".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::Claude), 1);
    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].client, "claude");
    assert_eq!(
        parsed.messages[0].session_id,
        "ses_123456789012345678901234567"
    );
    assert_eq!(parsed.messages[0].model_id, "claude-sonnet-4");
    assert_eq!(parsed.messages[0].input, 123);
    assert_eq!(parsed.messages[0].output, 45);
    assert_eq!(parsed.messages[0].cache_read, 67);
    assert_eq!(parsed.messages[0].cache_write, 8);
}
