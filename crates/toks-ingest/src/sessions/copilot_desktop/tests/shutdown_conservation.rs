use super::*;

/// Re-attribution only moves usage between days; it never creates or
/// destroys any. Summing every emitted message — the per-day increments
/// plus the remainder — reproduces the row's lifetime total exactly, with
/// `input + cache_read` compared against the row's input because the
/// normalizer moves the cached portion out of `input` into its own bucket.
///
/// This invariant is what makes the placement change safe to reconcile:
/// the day a token is credited to changes, the total does not.
#[test]
fn re_attribution_conserves_the_row_lifetime_total() {
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
            r#"{"type":"session.shutdown","data":{"shutdownType":"error","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":50,"cacheReadTokens":25,"cacheWriteTokens":0,"reasoningTokens":10}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":150,"outputTokens":75,"cacheReadTokens":40,"cacheWriteTokens":0,"reasoningTokens":15}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    let sum = |pick: fn(&UnifiedMessage) -> i64| -> i64 { messages.iter().map(pick).sum() };
    assert_eq!(
        sum(|message| message.tokens.input) + sum(|message| message.tokens.cache_read),
        200,
        "input is conserved once the cached portion is added back"
    );
    assert_eq!(sum(|message| message.tokens.output), 100);
    assert_eq!(sum(|message| message.tokens.cache_read), 50);
    assert_eq!(sum(|message| message.tokens.reasoning), 20);

    let mut days: Vec<i64> = messages.iter().map(|message| message.timestamp).collect();
    days.sort_unstable();
    assert_eq!(
        days,
        vec![1_782_909_296_000, 1_782_936_000_000, 1_782_950_400_000],
        "the same total is spread over the creation day and both shutdown days"
    );
}

/// A model's running peak, the verbatim-record dedup, and the dedup key the
/// message is submitted under all have to name the same model the message
/// is attributed to. They were keyed on the raw `modelMetrics` key while
/// the emitted `model_id` was trimmed, so `"gpt-5.1-codex"` and
/// `" gpt-5.1-codex "` landed on the same model with two separate peaks:
/// the later snapshot restated the earlier one's total instead of being
/// differenced against it, and the two records were submitted under keys
/// that differed only by invisible whitespace.
#[test]
fn model_spellings_that_differ_only_by_whitespace_share_one_snapshot_series() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    // The later snapshot restates the earlier one, so the row's lifetime
    // total is the final snapshot rather than their sum.
    insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 100, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            SESSION_START,
            r#"{"type":"session.shutdown","data":{"shutdownType":"error","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":50}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#,
            r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{" gpt-5.1-codex ":{"usage":{"inputTokens":200,"outputTokens":100}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    let total_input: i64 = messages.iter().map(|message| message.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|message| message.tokens.output).sum();
    assert_eq!(
        (total_input, total_output),
        (200, 100),
        "one peak: the padded spelling is the same model, so the second \
         snapshot restates the first instead of adding to it"
    );

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
        messages
            .iter()
            .all(|message| message.model_id == "gpt-5.1-codex"),
        "both spellings are attributed to the same model"
    );
    let mut keys: Vec<&str> = messages
        .iter()
        .filter_map(|message| message.dedup_key.as_deref())
        .collect();
    keys.sort_unstable();
    assert_eq!(
        keys,
        vec![
            "copilot-desktop:session-1:shutdown:9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01:gpt-5.1-codex",
            "copilot-desktop:session-1:shutdown:9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02:gpt-5.1-codex",
        ],
        "one dedup identity per model: the key names the model the message \
         is attributed to, not the raw spelling it happened to be written with"
    );
}

/// `events.jsonl` is not guaranteed append-only, and losing the head of the
/// file takes an earlier shutdown snapshot with it. The later record was
/// already submitted as an increment under a dedup key that does not
/// change, so differencing it from zero again would raise its day to the
/// full cumulative total while the earlier day keeps the usage it was
/// already credited with — permanently adding the rotated-away snapshot on
/// top of a total that was already complete.
///
/// The invariant: a record's emitted usage never grows because a snapshot
/// before it disappeared, and the usage it stands for is not re-emitted
/// somewhere else either.
#[test]
fn a_rotated_away_predecessor_does_not_grow_the_later_shutdown() {
    let first = r#"{"type":"session.shutdown","data":{"shutdownType":"error","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":100,"outputTokens":50}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a01","timestamp":"2026-07-01T20:00:00.000Z","parentId":null}"#;
    let second = r#"{"type":"session.shutdown","data":{"shutdownType":"routine","modelMetrics":{"gpt-5.1-codex":{"usage":{"inputTokens":200,"outputTokens":60}}}},"id":"9f2c6f0e-1d5a-4a7e-9b30-2c8d4e6f1a02","timestamp":"2026-07-02T00:00:00.000Z","parentId":null}"#;

    let parse = |lines: &[&str]| -> Vec<UnifiedMessage> {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("data.db");
        let conn = create_copilot_desktop_db(&db_path);
        // The later snapshot restates the earlier one, so the row's
        // lifetime total is the final snapshot rather than their sum.
        insert_session(&conn, "session-1", "gpt-5.1-codex", 200, 60, 0, 0);
        drop(conn);
        write_events(dir.path(), "session-1", lines);
        parse_copilot_desktop_db(&db_path)
    };
    let second_day = |messages: &[UnifiedMessage]| -> i64 {
        messages
            .iter()
            .find(|message| message.timestamp == 1_782_950_400_000)
            .map_or(0, |message| message.tokens.input)
    };

    let whole_history = parse(&[SESSION_START, first, second]);
    assert_eq!(
        second_day(&whole_history),
        100,
        "with both snapshots the later record is only its own increment"
    );

    // Compaction: the head of the log is gone, so `session.start` and the
    // earlier snapshot went with it and only the later record survives.
    let after_compaction = parse(&[second]);

    assert!(
        second_day(&after_compaction) <= 100,
        "the later record must not grow into the full cumulative total when \
         its predecessor is rotated away; it reported {}",
        second_day(&after_compaction)
    );
    let emitted: i64 = after_compaction
        .iter()
        .map(|message| message.tokens.total())
        .sum();
    assert_eq!(
        emitted, 0,
        "the surviving snapshot restates usage that is already attributed, \
         so it is not re-emitted on the creation day either"
    );
}
