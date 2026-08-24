use super::*;

#[test]
fn test_parse_zcode_sqlite_model_usage() {
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        "INSERT INTO session (id, directory, path) VALUES (?1, ?2, ?3)",
        params!["sess_1", "/Users/alice/work/demo", "/Users/alice/work/demo"],
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, turn_id, model_id, started_at, completed_at,
            duration_ms, input_tokens, output_tokens, reasoning_tokens,
            cache_read_input_tokens, cache_creation_input_tokens, computed_total_tokens, agent, mode
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
        "#,
        params![
            "usage_1",
            "sess_1",
            "turn_1",
            "GLM-5.2",
            1_782_718_000_000_i64,
            1_782_718_001_000_i64,
            1000_i64,
            100_i64,
            20_i64,
            5_i64,
            7_i64,
            3_i64,
            120_i64,
            "zcode-agent",
            "yolo",
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.client, "zcode");
    assert_eq!(msg.provider_id, "zhipu");
    assert_eq!(msg.model_id, "glm-5.2");
    assert_eq!(msg.session_id, "sess_1");
    // Timestamp anchors to `started_at` (the call's start), not
    // `completed_at` (the call's end). See #890 (follow-up).
    assert_eq!(msg.timestamp, 1_782_718_000_000_i64);
    assert_eq!(msg.duration_ms, Some(1000));
    assert_eq!(msg.tokens.input, 90);
    assert_eq!(msg.tokens.output, 15);
    assert_eq!(msg.tokens.reasoning, 5);
    assert_eq!(msg.tokens.cache_read, 7);
    assert_eq!(msg.tokens.cache_write, 3);
    assert_eq!(msg.agent.as_deref(), Some("zcode-agent"));
    assert_eq!(msg.workspace_key.as_deref(), Some("/Users/alice/work/demo"));
    assert_eq!(msg.workspace_label.as_deref(), Some("demo"));
    assert!(msg.is_turn_start);
    assert_eq!(msg.dedup_key.as_deref(), Some("zcode-sqlite:usage_1"));
}

#[test]
fn test_parse_zcode_sqlite_marks_only_first_request_per_turn() {
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    for (id, completed_at) in [("usage_1", 1_000_i64), ("usage_2", 2_000_i64)] {
        conn.execute(
            r#"
            INSERT INTO model_usage (
                id, session_id, turn_id, model_id, completed_at,
                input_tokens, output_tokens
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            "#,
            params![
                id,
                "sess_1",
                "turn_1",
                "glm-5.2",
                completed_at,
                10_i64,
                1_i64
            ],
        )
        .unwrap();
    }

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 2);
    assert!(messages[0].is_turn_start);
    assert!(!messages[1].is_turn_start);
}

#[test]
fn test_model_usage_timestamp_is_start_anchored() {
    // Regression (follow-up to #890): `model_usage` records both
    // `started_at` and `completed_at` for a call, plus an explicit
    // `duration_ms`. Anchoring the message timestamp at `completed_at`
    // would make sessionize()'s `[timestamp, timestamp + duration_ms]`
    // span project forward past the actual completion into phantom idle
    // time. The parser must prefer `started_at`.
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, turn_id, model_id, started_at, completed_at,
            duration_ms, input_tokens, output_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            "usage_1",
            "sess_1",
            "turn_1",
            "glm-5.2",
            1_782_718_000_000_i64,
            1_782_718_005_000_i64,
            5000_i64,
            10_i64,
            1_i64,
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].timestamp, 1_782_718_000_000_i64,
        "timestamp must anchor at started_at, not completed_at"
    );
    assert_eq!(
        messages[0].duration_ms,
        Some(5000),
        "duration_ms must still span from start to completion"
    );
}

#[test]
fn test_model_usage_missing_started_at_back_calculates_from_completed_at() {
    // Second-round review fix: when `started_at` is NULL but
    // `completed_at` and a positive `duration_ms` are present, the row
    // must not stay end-anchored at `completed_at` (a phantom forward
    // projection past the call's actual completion). Back-calculate the
    // start anchor from `completed_at - duration_ms` instead.
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, turn_id, model_id, completed_at,
            duration_ms, input_tokens, output_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            "usage_1",
            "sess_1",
            "turn_1",
            "glm-5.2",
            1_782_718_005_000_i64,
            5000_i64,
            10_i64,
            1_i64,
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].timestamp, 1_782_718_000_000_i64,
        "timestamp must be back-calculated from completed_at - duration_ms when started_at is missing"
    );
    assert_eq!(messages[0].duration_ms, Some(5000));
}
