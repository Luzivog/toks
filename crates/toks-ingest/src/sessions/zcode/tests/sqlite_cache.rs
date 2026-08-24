use super::*;

#[test]
fn test_parse_zcode_sqlite_cache_inclusive_normalization() {
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, model_id, completed_at,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_input_tokens, cache_creation_input_tokens, computed_total_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            "usage_cache_incl",
            "sess_cache",
            "glm-5.2",
            1_000_i64,
            100_i64,
            50_i64,
            10_i64,
            80_i64,
            5_i64,
            150_i64,
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.tokens.input, 15);
    assert_eq!(msg.tokens.output, 40);
    assert_eq!(msg.tokens.cache_read, 80);
    assert_eq!(msg.tokens.cache_write, 5);
    assert_eq!(msg.tokens.reasoning, 10);
    assert_eq!(msg.tokens.total(), 150);
}

#[test]
fn test_parse_zcode_sqlite_legacy_schema_subtracts_unconditionally() {
    // True legacy schema: no `computed_total_tokens` column (and no
    // `session` table), so the column probe and the modern query both
    // fail and the legacy fallback runs with is_legacy_schema=true.
    // Every row must then take the unconditional-subtraction branch.
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("db.sqlite");
    let conn = Connection::open(&db_path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE model_usage (
            id TEXT PRIMARY KEY,
            session_id TEXT,
            turn_id TEXT,
            model_id TEXT,
            started_at INTEGER,
            completed_at INTEGER,
            duration_ms INTEGER,
            input_tokens INTEGER,
            output_tokens INTEGER,
            reasoning_tokens INTEGER,
            cache_read_input_tokens INTEGER,
            cache_creation_input_tokens INTEGER,
            agent TEXT,
            mode TEXT
        );
        "#,
    )
    .unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, model_id, completed_at,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_input_tokens, cache_creation_input_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            "usage_legacy",
            "sess_legacy",
            "glm-5.2",
            1_000_i64,
            100_i64,
            50_i64,
            10_i64,
            80_i64,
            5_i64,
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.tokens.input, 15);
    assert_eq!(msg.tokens.output, 40);
    assert_eq!(msg.tokens.cache_read, 80);
    assert_eq!(msg.tokens.cache_write, 5);
    assert_eq!(msg.tokens.reasoning, 10);
    assert_eq!(msg.tokens.total(), 150);
}

#[test]
fn test_parse_zcode_sqlite_modern_schema_null_total_passes_through() {
    // Modern schema (computed_total_tokens column exists) but this row's
    // value is NULL: the shape can't be detected, so input/output must
    // pass through unchanged rather than being unconditionally subtracted.
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, model_id, completed_at,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_input_tokens, cache_creation_input_tokens, computed_total_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, NULL)
        "#,
        params![
            "usage_null_total",
            "sess_null",
            "glm-5.2",
            1_000_i64,
            100_i64,
            50_i64,
            10_i64,
            80_i64,
            5_i64,
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.tokens.input, 100);
    assert_eq!(msg.tokens.output, 50);
    assert_eq!(msg.tokens.cache_read, 80);
    assert_eq!(msg.tokens.cache_write, 5);
    assert_eq!(msg.tokens.reasoning, 10);
}

#[test]
fn test_parse_zcode_sqlite_cache_exclusive_preserved() {
    let dir = TempDir::new().unwrap();
    let db_path = create_zcode_sqlite_db(&dir);
    let conn = Connection::open(&db_path).unwrap();
    conn.execute(
        r#"
        INSERT INTO model_usage (
            id, session_id, model_id, completed_at,
            input_tokens, output_tokens, reasoning_tokens,
            cache_read_input_tokens, cache_creation_input_tokens, computed_total_tokens
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            "usage_cache_excl",
            "sess_excl",
            "claude-sonnet-5",
            1_000_i64,
            20_i64,
            30_i64,
            5_i64,
            80_i64,
            10_i64,
            145_i64,
        ],
    )
    .unwrap();

    let messages = parse_zcode_sqlite(&db_path);

    assert_eq!(messages.len(), 1);
    let msg = &messages[0];
    assert_eq!(msg.tokens.input, 20);
    assert_eq!(msg.tokens.output, 30);
    assert_eq!(msg.tokens.cache_read, 80);
    assert_eq!(msg.tokens.cache_write, 10);
    assert_eq!(msg.tokens.reasoning, 5);
    assert_eq!(msg.tokens.total(), 145);
}
