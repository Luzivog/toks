use super::*;
use std::io::Write;
use tempfile::TempDir;

/// Write the given JSONL `content` to `events.jsonl` inside a session
/// directory whose name drives `session_id_from_path`, then parse it.
fn parse_events(content: &str) -> Vec<UnifiedMessage> {
    let dir = TempDir::new().unwrap();
    let session_dir = dir.path().join("session-250622-101010");
    std::fs::create_dir_all(&session_dir).unwrap();
    let path = session_dir.join("events.jsonl");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    parse_junie_file(&path)
}

fn usage_event(timestamp_ms: i64, model: &str, input: i64, output: i64) -> String {
    format!(
        r#"{{"timestampMs":{timestamp_ms},"event":{{"agentEvent":{{"kind":"LlmResponseMetadataEvent","modelUsage":[{{"model":"{model}","inputTokens":{input},"outputTokens":{output}}}]}}}}}}"#
    )
}

#[test]
fn distinct_usage_rows_with_identical_tokens_are_both_counted() {
    // Two separate LLM response events with identical token counts but
    // distinct `timestampMs` (the realistic shape of #727: back-to-back
    // calls returning the same usage). Both must be counted. The original
    // #727 bug dropped the second because the per-`modelUsage` index reset
    // to 0; here the differing timestamp keeps the keys distinct.
    let content = format!(
        "{}\n{}\n",
        usage_event(1_750_000_000_000, "gpt-5", 100, 50),
        usage_event(1_750_000_001_000, "gpt-5", 100, 50),
    );
    let messages = parse_events(&content);

    assert_eq!(
        messages.len(),
        2,
        "both distinct calls with identical token counts must be counted"
    );
    for message in &messages {
        assert_eq!(message.tokens.input, 100);
        assert_eq!(message.tokens.output, 50);
    }
    assert_ne!(
        messages[0].dedup_key, messages[1].dedup_key,
        "distinct usage rows must receive distinct dedup keys"
    );
}

#[test]
fn field_present_cost_is_provider_reported_even_when_zero() {
    let content = r#"{"timestampMs":1750000000000,"event":{"agentEvent":{"kind":"LlmResponseMetadataEvent","modelUsage":[{"model":"unknown-model","inputTokens":1,"outputTokens":0,"cost":0}]}}}"#;
    let messages = parse_events(content);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].cost, 0.0);
    assert_eq!(
        messages[0].cost_source,
        super::super::CostSource::ProviderReported
    );
}

#[test]
fn replayed_identical_event_is_deduplicated_to_one() {
    // Junie can append/replay the exact same `LlmResponseMetadataEvent`.
    // A byte-for-byte replayed event (same timestamp, model, and tokens)
    // must collapse to a single counted row — otherwise the same tokens
    // and cost are double-counted. The dedup suffix is derived from the
    // event's own within-array index, so the replay reproduces the same
    // dedup key and is dropped by the `seen` set.
    let content = format!(
        "{}\n{}\n",
        usage_event(1_750_000_000_000, "gpt-5", 100, 50),
        usage_event(1_750_000_000_000, "gpt-5", 100, 50),
    );
    let messages = parse_events(&content);

    assert_eq!(
        messages.len(),
        1,
        "a replayed identical usage event must collapse to a single row"
    );
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
}

#[test]
fn identical_rows_within_one_event_are_both_counted() {
    // Multiple identical rows inside a single `modelUsage` array must also
    // each survive: they get distinct within-event indices (0 and 1).
    let content = "{\"timestampMs\":1750000000000,\"event\":{\"agentEvent\":{\"kind\":\"LlmResponseMetadataEvent\",\"modelUsage\":[{\"model\":\"gpt-5\",\"inputTokens\":100,\"outputTokens\":50},{\"model\":\"gpt-5\",\"inputTokens\":100,\"outputTokens\":50}]}}}\n";
    let messages = parse_events(content);
    assert_eq!(messages.len(), 2);
    assert_ne!(messages[0].dedup_key, messages[1].dedup_key);
}

#[test]
fn pending_turn_start_does_not_leak_when_prompt_yields_no_usage() {
    // Prompt A opens a turn but its response event carries no counted usage
    // (zero tokens). Prompt B then opens its own turn with real usage. The
    // turn-start must attach to B's usage, and the empty A response must not
    // leak the flag onto an unrelated later usage event.
    let empty_usage = r#"{"timestampMs":1750000000000,"event":{"agentEvent":{"kind":"LlmResponseMetadataEvent","modelUsage":[{"model":"gpt-5","inputTokens":0,"outputTokens":0}]}}}"#;
    let content = format!(
        "{}\n{}\n{}\n{}\n",
        r#"{"kind":"UserPromptEvent"}"#,
        empty_usage,
        r#"{"kind":"UserPromptEvent"}"#,
        usage_event(1_750_000_100_000, "gpt-5", 100, 50),
    );
    let messages = parse_events(&content);

    assert_eq!(messages.len(), 1);
    assert!(
        messages[0].is_turn_start,
        "turn-start should attach to prompt B's real usage"
    );
}

#[test]
fn turn_start_marks_only_the_first_usage_after_a_prompt() {
    let content = format!(
        "{}\n{}\n{}\n",
        r#"{"kind":"UserPromptEvent"}"#,
        usage_event(1_750_000_000_000, "gpt-5", 100, 50),
        usage_event(1_750_000_100_000, "gpt-5", 200, 60),
    );
    let messages = parse_events(&content);

    assert_eq!(messages.len(), 2);
    assert!(messages[0].is_turn_start);
    assert!(
        !messages[1].is_turn_start,
        "only the first usage event after a prompt is a turn-start"
    );
}

#[test]
fn usage_line_mentioning_skipped_kind_is_not_dropped() {
    // The user prompt text legitimately mentions a skipped kind name; the
    // following usage event must still be counted because the skip decision
    // is made on the parsed event kind, not a raw substring match.
    let content = format!(
        "{}\n{}\n",
        r#"{"kind":"UserPromptEvent","prompt":"please review the AgentStateUpdatedEvent handling"}"#,
        usage_event(1_750_000_000_000, "gpt-5", 100, 50),
    );
    let messages = parse_events(&content);

    assert_eq!(
        messages.len(),
        1,
        "a usage event must not be dropped just because a prior line mentioned a skipped kind"
    );
    assert!(messages[0].is_turn_start);
}

#[test]
fn skipped_event_kind_is_ignored() {
    let content = format!(
        "{}\n{}\n",
        r#"{"kind":"AgentStateUpdatedEvent","event":{"agentEvent":{"kind":"LlmResponseMetadataEvent","modelUsage":[{"model":"gpt-5","inputTokens":100,"outputTokens":50}]}}}"#,
        usage_event(1_750_000_000_000, "gpt-5", 100, 50),
    );
    let messages = parse_events(&content);
    // Only the genuine usage event counts; the snapshot tagged with a
    // skipped top-level kind is ignored even though it embeds a usage shape.
    assert_eq!(messages.len(), 1);
}

#[test]
fn test_usage_timestamp_is_start_anchored() {
    // Regression (follow-up to #890): `timestampMs` on a
    // LlmResponseMetadataEvent is recorded when the response is logged
    // (the call's *end*), and `usage.time` is that call's latency. If the
    // message's timestamp were left at `timestampMs`, sessionize()'s
    // `[timestamp, timestamp + duration_ms]` span would project forward
    // past the actual completion into phantom idle time. The parser must
    // back-calculate the start anchor instead.
    let content = r#"{"timestampMs":1750000005000,"event":{"agentEvent":{"kind":"LlmResponseMetadataEvent","modelUsage":[{"model":"gpt-5","inputTokens":100,"outputTokens":50,"time":2000}]}}}"#;
    let messages = parse_events(content);

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].timestamp, 1_750_000_003_000,
        "timestamp must be back-calculated to the call start (end - duration)"
    );
    assert_eq!(
        messages[0].duration_ms,
        Some(2000),
        "duration_ms must still span from start to the logged end timestamp"
    );
}

#[test]
fn missing_timestamp_ms_does_not_subtract_from_session_fallback() {
    // Second-round review fix: when `timestampMs` is absent (only
    // `usage.time` latency is recorded), `timestamp` falls back to
    // `default_timestamp` (session-ID-derived, or file mtime) — not a
    // per-event recorded end time. Back-calculating
    // `default_timestamp - usage.time` in that case would shift the
    // message into the wrong day rather than anchor it correctly, since
    // the fallback was never the call's actual completion time.
    let content = r#"{"event":{"agentEvent":{"kind":"LlmResponseMetadataEvent","modelUsage":[{"model":"gpt-5","inputTokens":100,"outputTokens":50,"time":2000}]}}}"#;
    let messages = parse_events(content);

    assert_eq!(messages.len(), 1);
    let expected_fallback = session_timestamp_from_id("session-250622-101010").unwrap();
    assert_eq!(
        messages[0].timestamp, expected_fallback,
        "timestamp must stay at the session-derived fallback, not be back-calculated from it"
    );
    assert_eq!(messages[0].duration_ms, Some(2000));
}
