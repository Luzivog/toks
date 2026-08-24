use super::*;

#[test]
fn shutdown_events_attribute_usage_to_their_own_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 100, 50, 25, 10);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"requests":{"count":1,"cost":1},"usage":{"inputTokens":100,"outputTokens":50,"cacheReadTokens":25,"cacheWriteTokens":0,"reasoningTokens":10}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01"}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1, "the row total is fully accounted for");
    let message = &messages[0];
    assert_eq!(
        message.timestamp, 1_782_950_400_000,
        "usage belongs to the shutdown day, not the creation day"
    );
    assert_eq!(message.model_id, "gpt-5.1-codex");
    assert_eq!(message.tokens.input, 75);
    assert_eq!(message.tokens.output, 50);
    assert_eq!(message.tokens.cache_read, 25);
    assert_eq!(message.tokens.reasoning, 10);
    assert_eq!(
        message.dedup_key.as_deref(),
        Some(
            "copilot-desktop:session-1:shutdown:9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02:gpt-5.1-codex"
        ),
        "the shutdown message is keyed by the event's own id"
    );
}

/// Whatever the shutdown records do not account for still has to be kept,
/// so the row stays the authority on the all-time total when a run dies
/// before it can write its shutdown.
#[test]
fn usage_beyond_the_shutdown_events_stays_at_session_creation() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 100, 50, 20);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"requests":{"count":1,"cost":1},"usage":{"inputTokens":100,"outputTokens":50,"cacheReadTokens":25,"cacheWriteTokens":0,"reasoningTokens":10}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01"}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 2);
    let residual = messages
        .iter()
        .find(|message| message.timestamp == 1_782_909_296_000)
        .expect("the unaccounted remainder stays on the creation day");
    assert_eq!(residual.tokens.input, 75);
    assert_eq!(residual.tokens.output, 50);
    assert_eq!(residual.tokens.cache_read, 25);
    assert_eq!(residual.tokens.reasoning, 10);
    assert_eq!(
        residual.dedup_key.as_deref(),
        Some("copilot-desktop:session-1"),
        "the remainder keeps the row's own dedup key"
    );

    let total_input: i64 = messages.iter().map(|message| message.tokens.input).sum();
    assert_eq!(total_input, 150, "the row total is preserved exactly");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.message_count)
            .sum::<i32>(),
        1,
        "a shutdown plus residual still represents one SQLite session"
    );
}

/// The `sessions` table has no cache-write column, so that bucket was
/// hardcoded to zero. The shutdown records do carry it.
#[test]
fn shutdown_events_recover_cache_write_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 100, 50, 25, 10);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"requests":{"count":1,"cost":1},"usage":{"inputTokens":100,"outputTokens":50,"cacheReadTokens":25,"cacheWriteTokens":7,"reasoningTokens":10}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01"}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    let shutdown = messages
        .iter()
        .find(|message| message.timestamp == 1_782_950_400_000)
        .expect("shutdown message");
    assert_eq!(shutdown.tokens.cache_write, 7);
}

/// `modelMetrics` is keyed by model, which attributes each model's usage
/// exactly instead of letting the last `session.model_change` claim the
/// whole session.
#[test]
fn shutdown_events_split_usage_per_model() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "auto", 300, 60, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":20}},"claude-sonnet-4-5":{"usage":{"inputTokens":200,"outputTokens":40}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01"}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    let codex = messages
        .iter()
        .find(|message| message.model_id == "gpt-5.1-codex")
        .expect("codex row");
    let claude = messages
        .iter()
        .find(|message| message.model_id == "claude-sonnet-4-5")
        .expect("claude row");
    assert_eq!(codex.tokens.input, 100);
    assert_eq!(codex.provider_id, "openai");
    assert_eq!(claude.tokens.input, 200);
    assert_eq!(claude.provider_id, "anthropic");
    assert_eq!(
        messages
            .iter()
            .map(|message| message.message_count)
            .sum::<i32>(),
        1,
        "splitting one session across models must not inflate its count"
    );
}

/// `currentModel` belongs to each shutdown payload. Using the final session
/// model for every `auto` tracker fragment would move an earlier run to a
/// model selected only later.
#[test]
fn auto_shutdowns_keep_the_model_active_for_each_run() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "auto", 200, 0, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"currentModel":"gpt-5.1-codex","modelMetrics":{"auto":{"usage":{"inputTokens":100}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
            r#"{"type":"session.shutdown","data":{"currentModel":"claude-sonnet-4-5","modelMetrics":{"auto":{"usage":{"inputTokens":200}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);
    let first = messages
        .iter()
        .find(|message| message.timestamp == 1_782_936_000_000)
        .expect("first shutdown");
    let second = messages
        .iter()
        .find(|message| message.timestamp == 1_782_950_400_000)
        .expect("second shutdown");

    assert_eq!(
        (first.model_id.as_str(), first.tokens.input),
        ("gpt-5.1-codex", 100)
    );
    assert_eq!(
        (second.model_id.as_str(), second.tokens.input),
        ("claude-sonnet-4-5", 100)
    );
}

/// A `session.shutdown` record captured verbatim from a real
/// `~/.copilot/session-state/<id>/events.jsonl` on macOS (Copilot CLI
/// 1.0.25), with only the two UUIDs replaced. It pins the shape the desktop
/// app actually writes: `timestamp` is an ISO-8601 string on the envelope
/// next to `id`/`parentId`, `modelMetrics` is nested under `data`, and the
/// usage bucket carries `cacheWriteTokens`. Reading the timestamp from a
/// `ts` key under `data` finds nothing and drops the record.
#[test]
fn real_shutdown_record_attributes_usage_to_its_own_timestamp() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.4", 21_067, 29, 19_968, 22);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","totalPremiumRequests":1,"totalApiDurationMs":2970,"sessionStartTime":1776192215193,"codeChanges":{"linesAdded":0,"linesRemoved":0,"filesModified":[]},"modelMetrics":{"gpt-5.4":{"requests":{"count":1,"cost":1},"usage":{"inputTokens":21067,"outputTokens":29,"cacheReadTokens":19968,"cacheWriteTokens":0,"reasoningTokens":22}}},"currentModel":"gpt-5.4","currentTokens":22592,"systemTokens":9923,"conversationTokens":83,"toolDefinitionsTokens":12583},"id":"c1a4b7e2-90d3-4f61-8ba5-7d2e6f0c9134","timestamp":"2026-04-14T18:43:44.922Z","parentId":"5b8f3d10-2c47-4e89-a6f0-11d9c4e78a25"}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1, "the row total is fully accounted for");
    let message = &messages[0];
    assert_eq!(
        message.timestamp, 1_776_192_224_922,
        "the envelope timestamp is the run's own time, not `created_at`"
    );
    assert_eq!(message.model_id, "gpt-5.4");
    assert_eq!(message.tokens.input, 1_099);
    assert_eq!(message.tokens.output, 29);
    assert_eq!(message.tokens.cache_read, 19_968);
    assert_eq!(message.tokens.reasoning, 22);
    assert_eq!(
        message.dedup_key.as_deref(),
        Some("copilot-desktop:session-1:shutdown:c1a4b7e2-90d3-4f61-8ba5-7d2e6f0c9134:gpt-5.4")
    );
}

/// The dedup key has to identify the record, not its offset. Keying on the
/// enumeration index holds only while `events.jsonl` is strictly
/// append-only: rotate, truncate, or compact away an earlier shutdown and
/// every later index shifts down, so usage that was already submitted comes
/// back under a fresh key and the server counts it twice.
#[test]
fn shutdown_dedup_key_survives_an_earlier_shutdown_being_rotated_away() {
    let first = r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":50}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#;
    let second = r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":200,"outputTokens":60}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#;

    let key_of_second = |lines: &[&str]| -> (String, i64) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("data.db");
        let conn = create_copilot_desktop_db(&db_path);
        // The later snapshot restates the earlier one, so the row's
        // lifetime total is the final snapshot rather than their sum.
        insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 60, 0, 0);
        drop(conn);
        write_events(dir.path(), "session-1", lines);

        let messages = parse_copilot_desktop_db(&db_path);
        let key = messages
            .iter()
            .find(|message| message.timestamp == 1_782_950_400_000)
            .and_then(|message| message.dedup_key.clone())
            .expect("the second shutdown is always emitted");
        let total_input = messages.iter().map(|message| message.tokens.input).sum();
        (key, total_input)
    };

    let whole_history = key_of_second(&[SESSION_START, first, second]);
    // The earlier shutdown is gone but the log still opens where the
    // session did, so the survivor is still differenced and emitted; what
    // must not change is the key it is emitted under.
    let after_rotation = key_of_second(&[SESSION_START, second]);

    assert_eq!(
        whole_history.0, after_rotation.0,
        "dropping the earlier shutdown must not re-key the later one"
    );
    assert_eq!(
        whole_history.0,
        "copilot-desktop:session-1:shutdown:9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02:gpt-5.1-codex"
    );
    assert_eq!(whole_history.1, 200);
    assert_eq!(
        after_rotation.1, 200,
        "even a synthetic selective edit cannot exceed the authoritative row total"
    );
}
