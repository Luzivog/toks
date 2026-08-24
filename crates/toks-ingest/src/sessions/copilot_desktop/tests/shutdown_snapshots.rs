use super::*;

/// `session.shutdown` reports the tracker's running total, not the spend
/// since the last shutdown: the SDK's `UsageMetricsTracker` only ever adds
/// to its per-model counters and never resets, and `Session.shutdown()`
/// emits `modelMetrics` as-is with no one-shot guard, so an error shutdown
/// followed by a routine one writes two snapshots of the same total.
/// Summing them counted the earlier snapshot twice and dated the phantom
/// tokens to a day they were never spent on.
#[test]
fn cumulative_shutdown_snapshots_are_differenced_not_summed() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 100, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"error","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":50}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":200,"outputTokens":100}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    let total_input: i64 = messages.iter().map(|message| message.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|message| message.tokens.output).sum();
    assert_eq!(
        (total_input, total_output),
        (200, 100),
        "the second snapshot restates the first; summing them would report 300/150"
    );

    let first = messages
        .iter()
        .find(|message| message.timestamp == 1_782_936_000_000)
        .expect("the first shutdown keeps its own day");
    assert_eq!((first.tokens.input, first.tokens.output), (100, 50));

    let second = messages
        .iter()
        .find(|message| message.timestamp == 1_782_950_400_000)
        .expect("the second shutdown keeps its own day");
    assert_eq!(
        (second.tokens.input, second.tokens.output),
        (100, 50),
        "only the increment accrued since the previous snapshot"
    );

    assert!(
        !messages
            .iter()
            .any(|message| message.dedup_key.as_deref() == Some("copilot-desktop:session-1")),
        "the snapshots account for the whole row, so there is no remainder"
    );
}

/// Distinct legacy/malformed records can share a millisecond. Their
/// fallback identity must come from stable content rather than timestamp or
/// mutable file position.
#[test]
fn idless_shutdowns_at_the_same_timestamp_keep_distinct_stable_keys() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 0, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100}}}},"timestamp":"2026-07-02T00:00:00.000Z","parentId":"parent-a"}"#,
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":200}}}},"timestamp":"2026-07-02T00:00:00.000Z","parentId":"parent-b"}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);
    let keys: HashSet<&str> = messages
        .iter()
        .filter_map(|message| message.dedup_key.as_deref())
        .collect();

    assert_eq!(messages.len(), 2);
    assert_eq!(keys.len(), 2);
    assert!(keys.iter().all(|key| key.contains(":shutdown:anon-")));
}

/// A record written twice — a replayed or re-flushed log — describes one
/// shutdown. Keying on the event id is only half the fix: the parser also
/// has to collapse the repeat before it reads the numbers off it.
#[test]
fn a_repeated_shutdown_record_counts_once() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 100, 50, 0, 0);
    drop(conn);
    let record = r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":50}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#;
    write_events(dir.path(), "session-1", &[SESSION_START, record, record]);

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1, "the repeated record is one shutdown");
    assert_eq!(messages[0].timestamp, 1_782_950_400_000);
    assert_eq!(
        (messages[0].tokens.input, messages[0].tokens.output),
        (100, 50)
    );
}

/// A snapshot lower than the one before it means the tracker started over
/// with a fresh session object, or the records were read out of order.
/// Either way the difference is not negative usage, and it must not be
/// added on top of the peak already attributed.
#[test]
fn a_shutdown_snapshot_that_decreases_adds_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 100, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":200,"outputTokens":100}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":50,"outputTokens":20}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    assert!(
        messages.iter().all(|message| message.tokens.input >= 0
            && message.tokens.output >= 0
            && message.tokens.cache_read >= 0
            && message.tokens.cache_write >= 0
            && message.tokens.reasoning >= 0),
        "a lower snapshot must never produce a negative bucket"
    );

    let total_input: i64 = messages.iter().map(|message| message.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|message| message.tokens.output).sum();
    assert_eq!(
        (total_input, total_output),
        (200, 100),
        "the row total is the authority; the lower snapshot adds nothing"
    );

    assert!(
        !messages
            .iter()
            .any(|message| message.timestamp == 1_782_950_400_000),
        "a snapshot that explains no new usage is not emitted at all"
    );
}

/// The sidecar can be flushed before SQLite. A temporarily newer shutdown
/// must not exceed the row's authoritative lifetime buckets.
#[test]
fn shutdown_usage_is_bounded_by_the_sqlite_row() {
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
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":200,"outputTokens":100,"cacheReadTokens":80,"cacheWriteTokens":7,"reasoningTokens":20}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 75);
    assert_eq!(messages[0].tokens.cache_read, 25);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.reasoning, 10);
    assert_eq!(messages[0].tokens.cache_write, 7);
}

/// Cache is a subset of inclusive input. A sidecar that accounts for all
/// row input but temporarily omits cache metadata cannot leave a cache-only
/// residual that pushes normalized usage above the row total.
#[test]
fn residual_cache_is_bounded_by_residual_inclusive_input() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 100, 0, 50, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"cacheReadTokens":0}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);
    let normalized_input: i64 = messages
        .iter()
        .map(|message| message.tokens.input + message.tokens.cache_read)
        .sum();

    assert_eq!(normalized_input, 100);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.cache_read)
            .sum::<i64>(),
        0
    );
}

/// Inclusive input and cache reads are not independent totals. A reset or
/// out-of-order snapshot may lower inclusive input while raising the cache
/// sub-bucket, but that cannot authorize more lifetime input usage.
#[test]
fn cache_growth_without_inclusive_input_growth_does_not_mint_tokens() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 100, 0, 90, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"cacheReadTokens":80}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
            r#"{"type":"session.shutdown","data":{"modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":90,"cacheReadTokens":90}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);
    let total_input: i64 = messages
        .iter()
        .map(|message| message.tokens.input + message.tokens.cache_read)
        .sum();

    assert_eq!(
        total_input, 100,
        "the row lifetime input remains authoritative"
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.cache_read)
            .sum::<i64>(),
        90,
        "the final cache high-water is preserved without increasing total input"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.timestamp == 1_782_950_400_000),
        "cache growth without inclusive input growth is not emitted"
    );
}
