use super::*;

#[test]
fn test_deserialize_v2_message_resolves_nested_model() {
    let mut bytes = V2_ASSISTANT_DATA.as_bytes().to_vec();
    let msg: OpenCodeMessage = simd_json::from_slice(&mut bytes).unwrap();

    assert_eq!(msg.role, None, "v2 payloads carry no role field");
    assert!(msg.is_assistant(), "missing role defaults to assistant");
    assert_eq!(msg.resolve_model_id().as_deref(), Some("claude-sonnet-4"));
    assert_eq!(msg.resolve_provider_id().as_deref(), Some("anthropic"));
    assert_eq!(msg.agent.as_deref(), Some("build"));
}

#[test]
fn test_top_level_model_id_takes_precedence_over_nested() {
    let json = r#"{
        "role": "assistant",
        "modelID": "top-level-model",
        "providerID": "top-level-provider",
        "model": { "id": "nested-model", "providerID": "nested-provider" },
        "tokens": { "input": 1, "output": 1, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
        "time": { "created": 1700000000000.0 }
    }"#;
    let mut bytes = json.as_bytes().to_vec();
    let msg: OpenCodeMessage = simd_json::from_slice(&mut bytes).unwrap();

    assert_eq!(msg.resolve_model_id().as_deref(), Some("top-level-model"));
    assert_eq!(
        msg.resolve_provider_id().as_deref(),
        Some("top-level-provider")
    );
}

#[test]
fn test_parse_v2_session_message_reads_tokens_and_workspace() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("opencode-next.db");

    let conn = create_opencode_v2_sqlite_db(&db_path);
    conn.execute(
        "INSERT INTO session (id, directory) VALUES (?1, ?2)",
        rusqlite::params!["ses_v2", "/Users/alice/opencode-v2-repo"],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["msg_v2_001", "ses_v2", "assistant", V2_ASSISTANT_DATA],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1, "v2 assistant row should be parsed");
    let msg = &messages[0];
    assert_eq!(msg.model_id, "claude-sonnet-4");
    assert_eq!(msg.provider_id, "anthropic");
    assert_eq!(msg.tokens.input, 5519);
    assert_eq!(msg.tokens.output, 20);
    assert_eq!(msg.tokens.reasoning, 23);
    assert_eq!(msg.tokens.cache_read, 100);
    assert_eq!(msg.tokens.cache_write, 50);
    assert_eq!(msg.duration_ms, Some(238));
    assert_eq!(
        msg.workspace_key.as_deref(),
        Some("/Users/alice/opencode-v2-repo"),
        "workspace should come from session.directory"
    );
    assert_eq!(msg.workspace_label.as_deref(), Some("opencode-v2-repo"));
    assert_eq!(
        msg.dedup_key.as_deref(),
        Some("msg_v2_001"),
        "v2 dedup_key falls back to the session_message row id"
    );
    assert_eq!(
        msg.cost_source,
        crate::sessions::CostSource::ProviderReported
    );
}

#[test]
fn test_parse_v2_skips_non_assistant_and_tokenless_rows() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("opencode-next.db");

    let conn = create_opencode_v2_sqlite_db(&db_path);
    let user_data = r#"{ "time": { "created": 1783882279705 }, "content": [] }"#;
    let tokenless =
        r#"{ "time": { "created": 1783882279705 }, "model": { "id": "m", "providerID": "p" } }"#;
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["msg_ok", "ses_v2", "assistant", V2_ASSISTANT_DATA],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["msg_user", "ses_v2", "user", user_data],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["msg_synthetic", "ses_v2", "synthetic", V2_ASSISTANT_DATA],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["msg_no_tokens", "ses_v2", "assistant", tokenless],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(
        messages.len(),
        1,
        "only the assistant row with tokens should parse"
    );
    assert_eq!(messages[0].dedup_key.as_deref(), Some("msg_ok"));
}

#[test]
fn test_parse_v2_negative_tokens_clamped() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("opencode-next.db");

    let conn = create_opencode_v2_sqlite_db(&db_path);
    let negative = r#"{
        "time": { "created": 1783882279705 },
        "model": { "id": "claude-sonnet-4", "providerID": "anthropic" },
        "cost": -1.0,
        "tokens": { "input": -100, "output": -50, "reasoning": -25, "cache": { "read": -200, "write": -10 } }
    }"#;
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["msg_neg", "ses_v2", "assistant", negative],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.tokens.input, 0);
    assert_eq!(msg.tokens.output, 0);
    assert_eq!(msg.tokens.reasoning, 0);
    assert_eq!(msg.tokens.cache_read, 0);
    assert_eq!(msg.tokens.cache_write, 0);
    assert!(msg.cost >= 0.0);
}

#[test]
fn test_parse_v2_deduplicates_forked_session_message_history() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("opencode-next.db");

    let conn = create_opencode_v2_sqlite_db(&db_path);
    // Same payload copied into a forked session must collapse to one entry.
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["root_row", "root_session", "assistant", V2_ASSISTANT_DATA],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["fork_row", "fork_session", "assistant", V2_ASSISTANT_DATA],
    )
    .unwrap();
    drop(conn);

    let messages = parse_opencode_sqlite(&db_path);
    assert_eq!(
        messages.len(),
        1,
        "forked copies of the same assistant turn collapse inside v2 parsing"
    );
}

#[test]
fn test_distinct_embedded_ids_are_not_merged_despite_fingerprint_collision() {
    // Two genuinely different assistant messages can share every fingerprint
    // field (timestamp, model, tokens, cost, agent). When both carry an
    // embedded `$.id` and the ids DIFFER, they are distinct messages -- not
    // fork copies -- and must be kept separate rather than collapsed.
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("opencode-next.db");
    let conn = create_opencode_v2_sqlite_db(&db_path);

    let payload = |id: &str| {
        format!(
            r#"{{
                "id": "{id}",
                "time": {{ "created": 1783882279705, "completed": 1783882279943 }},
                "agent": "build",
                "model": {{ "id": "claude-sonnet-4", "providerID": "anthropic" }},
                "cost": 0.0123,
                "tokens": {{ "input": 10, "output": 5, "reasoning": 0, "cache": {{ "read": 0, "write": 0 }} }}
            }}"#
        )
    };

    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["row_a", "ses_v2", "assistant", payload("msg_a")],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["row_b", "ses_v2", "assistant", payload("msg_b")],
    )
    .unwrap();
    // A true fork of msg_a (same embedded id, different session/row) must
    // still collapse into msg_a rather than becoming a third entry.
    conn.execute(
        "INSERT INTO session_message (id, session_id, type, data) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["row_a_fork", "fork_session", "assistant", payload("msg_a")],
    )
    .unwrap();
    drop(conn);

    let mut dedup_keys: Vec<String> = parse_opencode_sqlite(&db_path)
        .into_iter()
        .filter_map(|m| m.dedup_key)
        .collect();
    dedup_keys.sort();
    assert_eq!(
        dedup_keys,
        vec!["msg_a".to_string(), "msg_b".to_string()],
        "distinct embedded ids stay separate; a same-id fork collapses"
    );
}
