use super::*;

#[test]
fn test_deduplication_skips_duplicate_entries() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:02.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":100}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        2,
        "Should deduplicate to 2 messages (first duplicate skipped)"
    );
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[1].tokens.input, 200);
}

#[test]
fn test_parse_cc_mirror_claude_variant_attributes_client_provider_and_workspace() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50,"cache_read_input_tokens":10,"cache_creation_input_tokens":5}}}"#;

    let (_temp_dir, path) = create_cc_mirror_project_file(
        content,
        "zai-worker",
        "zai",
        "-Users-example-work",
        "session.jsonl",
    );

    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "cc-mirror/zai-worker");
    assert_eq!(messages[0].provider_id, "zai");
    assert_eq!(messages[0].model_id, "claude-3-5-sonnet");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 10);
    assert_eq!(messages[0].tokens.cache_write, 5);
    assert_eq!(
        messages[0].workspace_key.as_deref(),
        Some("-Users-example-work")
    );
    assert_eq!(
        messages[0].workspace_label.as_deref(),
        Some("-Users-example-work")
    );
}

#[test]
fn test_cc_mirror_variant_client_segment_is_submit_safe() {
    assert_eq!(sanitize_cc_mirror_segment(" zaicc "), "zaicc");
    assert_eq!(sanitize_cc_mirror_segment("../Zai CC!"), "zai-cc");
    assert_eq!(sanitize_cc_mirror_segment("..."), "variant");
    assert_eq!(sanitize_cc_mirror_segment(&"a".repeat(120)).len(), 96);
}

#[test]
fn test_deduplication_keeps_max_output_for_streaming_duplicates() {
    // CC streaming writes the same messageId:requestId multiple times.
    // The first entry has a partial output_tokens count; the last has the
    // final (largest) count. We must keep the entry with the highest
    // output_tokens, not the first-seen entry.
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":31}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":31}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.200Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":300}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        1,
        "Streaming duplicates should collapse to one entry"
    );
    assert_eq!(
        messages[0].tokens.output, 300,
        "Should keep the max output_tokens"
    );
    assert_eq!(messages[0].tokens.input, 10);
}

#[test]
fn test_deduplication_per_field_max_not_just_output() {
    // Later entry has same output but higher input - should still update input
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":100,"cache_read_input_tokens":5}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":50,"output_tokens":100,"cache_read_input_tokens":20}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.output, 100);
    assert_eq!(
        messages[0].tokens.input, 50,
        "Should keep max input even if output unchanged"
    );
    assert_eq!(
        messages[0].tokens.cache_read, 20,
        "Should keep max cache_read even if output unchanged"
    );
}

#[test]
fn test_deduplication_higher_first_lower_later() {
    // First entry has higher output than later - should keep first's higher values
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":500}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":100}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].tokens.output, 500,
        "Should keep max output (first entry)"
    );
    assert_eq!(
        messages[0].tokens.input, 100,
        "Should keep max input (first entry)"
    );
}

#[test]
fn test_deduplication_promotes_provider_hint_from_later_duplicate() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","provider":"openrouter/anthropic","model":"claude-3-5-sonnet","usage":{"input_tokens":120,"output_tokens":75}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openrouter");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 75);
}

#[test]
fn test_deduplication_promotes_provider_hint_without_later_model() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","provider":"openrouter/anthropic","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","usage":{"input_tokens":120,"output_tokens":75}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openrouter");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 75);
}

#[test]
fn test_deduplication_preserves_explicit_provider_against_later_inference() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","provider":"openrouter/anthropic","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":120,"output_tokens":75}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openrouter");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 75);
}

#[test]
fn test_deduplication_skips_model_none_without_stale_index() {
    // First entry has id+requestId+usage but model=null → skipped, no push.
    // Second entry is a valid duplicate. Must not panic on stale index.
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","usage":{"input_tokens":10,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:00.100Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":100}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        1,
        "Only the entry with model should be kept"
    );
    assert_eq!(messages[0].tokens.output, 100);
}

#[test]
fn test_deduplication_allows_same_message_different_request() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_002","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":150,"output_tokens":75}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        2,
        "Different requestId should not be deduplicated"
    );
}

#[test]
fn test_deduplication_uses_message_id_without_request_id_and_keeps_final_duration() {
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Hello"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","message":{"id":"msg_stream","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":25}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:03.500Z","message":{"id":"msg_stream","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":250}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.output, 250);
    assert_eq!(messages[0].timestamp, 1_733_047_200_000);
    assert_eq!(messages[0].duration_ms, Some(3500));
    assert_eq!(messages[0].dedup_key.as_deref(), Some("message:msg_stream"));
}

#[test]
fn test_dedup_merge_duration_is_monotonic_across_out_of_order_duplicates() {
    // Regression: several streaming duplicates of one message can be
    // processed out of order (e.g. a late-arriving chunk carrying an
    // earlier completion timestamp than one already merged). The start
    // anchor (existing.timestamp) must survive every merge, and
    // duration_ms must never shrink below a value already established by
    // an earlier-processed duplicate — it must track the latest
    // (largest) end timestamp seen so far.
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Hello"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_multi","message":{"id":"msg_multi","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":30}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:05.000Z","requestId":"req_multi","message":{"id":"msg_multi","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":100}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:02.000Z","requestId":"req_multi","message":{"id":"msg_multi","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:07.000Z","requestId":"req_multi","message":{"id":"msg_multi","model":"claude-3-5-sonnet","usage":{"input_tokens":10,"output_tokens":200}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        1,
        "all streaming duplicates should collapse to one message"
    );
    assert_eq!(
        messages[0].timestamp, 1_733_047_200_000,
        "the start anchor must survive every merge (the user entry's timestamp)"
    );
    assert_eq!(
        messages[0].duration_ms,
        Some(7_000),
        "duration_ms must equal the latest end timestamp minus the start \
             anchor (7s), not shrink when an out-of-order duplicate with an \
             earlier timestamp is merged"
    );
    assert_eq!(
        messages[0].tokens.output, 200,
        "token fields keep the per-field max across all duplicates"
    );
}

#[test]
fn test_pending_request_start_is_cleared_between_assistant_messages() {
    // Regression: previously, the user-entry timestamp was set into
    // `pending_request_start_timestamp_ms` and never cleared after the
    // first assistant message consumed it. A subsequent assistant message
    // with no intervening user entry would then reuse the stale start
    // timestamp and report a wildly inflated duration.
    let content = r#"{"type":"user","timestamp":"2024-12-01T10:00:00.000Z","message":{"content":"Hello"}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:01:30.000Z","requestId":"req_002","message":{"id":"msg_002","model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":80}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].duration_ms,
        Some(1_000),
        "first assistant should report duration vs the user entry (1s)"
    );
    assert_eq!(
        messages[1].duration_ms, None,
        "second assistant has no preceding user entry; duration must NOT \
             reuse the stale pending_request_start_timestamp_ms"
    );
}

#[test]
fn test_entries_without_dedup_fields_still_processed() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
{"type":"assistant","timestamp":"2024-12-01T10:00:01.000Z","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":200,"output_tokens":100}}}"#;

    let file = create_test_file(content);
    let messages = parse_claude_file(file.path());

    assert_eq!(
        messages.len(),
        2,
        "Entries without messageId/requestId should still be processed"
    );
}
