use super::*;

#[test]
fn test_parse_kimi_valid_status_update() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983426.420942, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 1562, "output": 2463, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "chatcmpl-xxx"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "kimi");
    assert_eq!(messages[0].model_id, "kimi-for-coding");
    assert_eq!(messages[0].provider_id, "moonshot");
    assert_eq!(messages[0].tokens.input, 1562);
    assert_eq!(messages[0].tokens.output, 2463);
    assert_eq!(messages[0].tokens.cache_read, 0);
    assert_eq!(messages[0].tokens.cache_write, 0);
    // Timestamp: 1770983426.420942 * 1000 = 1770983426420
    assert_eq!(messages[0].timestamp, 1770983426420);
}

#[test]
fn test_parse_kimi_multi_turn() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983400.0, "message": {"type": "TurnBegin", "payload": {"user_input": "hello"}}}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 200, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-1"}}}
{"timestamp": 1770983420.0, "message": {"type": "TurnBegin", "payload": {"user_input": "world"}}}
{"timestamp": 1770983430.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 300, "output": 400, "input_cache_read": 50, "input_cache_creation": 0}, "message_id": "msg-2"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 200);
    assert_eq!(messages[1].tokens.input, 300);
    assert_eq!(messages[1].tokens.output, 400);
    assert_eq!(messages[1].tokens.cache_read, 50);
}

#[test]
fn test_parse_kimi_skip_non_status_update() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983400.0, "message": {"type": "TurnBegin", "payload": {"user_input": "hello"}}}
{"timestamp": 1770983410.0, "message": {"type": "ContentPart", "payload": {"type": "text", "text": "response"}}}
{"timestamp": 1770983420.0, "message": {"type": "ToolCall", "payload": {"type": "function", "id": "tool_1", "function": {"name": "ReadFile", "arguments": "{}"}}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_parse_kimi_empty_file() {
    let file = create_test_file("");

    let messages = parse_kimi_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_parse_kimi_tool_call_multi_step() {
    // Simulates a tool-call scenario with multiple StatusUpdate messages in one turn
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983400.0, "message": {"type": "TurnBegin", "payload": {"user_input": "read file"}}}
{"timestamp": 1770983405.0, "message": {"type": "StepBegin", "payload": {"n": 1}}}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 500, "output": 100, "input_cache_read": 200, "input_cache_creation": 0}, "message_id": "msg-step1"}}}
{"timestamp": 1770983415.0, "message": {"type": "ToolCall", "payload": {"type": "function", "id": "tool_1", "function": {"name": "ReadFile", "arguments": "{}"}}}}
{"timestamp": 1770983420.0, "message": {"type": "StepBegin", "payload": {"n": 2}}}
{"timestamp": 1770983425.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 800, "output": 300, "input_cache_read": 400, "input_cache_creation": 100}, "message_id": "msg-step2"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 2);
    // Step 1
    assert_eq!(messages[0].tokens.input, 500);
    assert_eq!(messages[0].tokens.output, 100);
    assert_eq!(messages[0].tokens.cache_read, 200);
    assert_eq!(messages[0].tokens.cache_write, 0);
    // Step 2
    assert_eq!(messages[1].tokens.input, 800);
    assert_eq!(messages[1].tokens.output, 300);
    assert_eq!(messages[1].tokens.cache_read, 400);
    assert_eq!(messages[1].tokens.cache_write, 100);
}

#[test]
fn test_parse_kimi_with_cache_tokens() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1771123711.615454, "message": {"type": "StatusUpdate", "payload": {"context_usage": 0.024, "token_usage": {"input_other": 1508, "output": 205, "input_cache_read": 4864, "input_cache_creation": 0}, "message_id": "chatcmpl-2tNw2mhUNfdPMP0Jyie7gDhD"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 1508);
    assert_eq!(messages[0].tokens.output, 205);
    assert_eq!(messages[0].tokens.cache_read, 4864);
    assert_eq!(messages[0].tokens.cache_write, 0);
}

#[test]
fn test_parse_kimi_deduplicates_repeated_status_updates_by_message_id() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 10, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-progressive"}}}
{"timestamp": 1770983420.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 120, "output": 30, "input_cache_read": 5, "input_cache_creation": 0}, "message_id": "msg-progressive"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("msg-progressive"));
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 30);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].timestamp, 1770983420000);
}

#[test]
fn test_parse_kimi_keeps_larger_extreme_status_update() {
    // Both saturating totals equal i64::MAX, but the first exact total is larger.
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 9223372036854775807, "output": 9223372036854775807, "input_cache_read": 2, "input_cache_creation": 0}, "message_id": "msg-extreme"}}}
{"timestamp": 1770983420.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 9223372036854775807, "output": 0, "input_cache_read": 1, "input_cache_creation": 0}, "message_id": "msg-extreme"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("msg-extreme"));
    assert_eq!(messages[0].tokens.input, i64::MAX);
    assert_eq!(messages[0].tokens.output, i64::MAX);
    assert_eq!(messages[0].tokens.cache_read, 2);
    assert_eq!(messages[0].tokens.cache_write, 0);
    assert_eq!(messages[0].timestamp, 1770983410000);
}

#[test]
fn test_parse_kimi_keeps_distinct_and_missing_message_ids_separate() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 10, "output": 1, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-1"}}}
{"timestamp": 1770983420.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 20, "output": 2, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-2"}}}
{"timestamp": 1770983430.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 30, "output": 3, "input_cache_read": 0, "input_cache_creation": 0}}}}
{"timestamp": 1770983440.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 40, "output": 4, "input_cache_read": 0, "input_cache_creation": 0}}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("msg-1"));
    assert_eq!(messages[1].dedup_key.as_deref(), Some("msg-2"));
    assert!(messages[2].dedup_key.is_none());
    assert!(messages[3].dedup_key.is_none());
}

#[test]
fn test_parse_kimi_skips_zero_token_entries() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 0, "output": 0, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-empty"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_parse_kimi_keeps_extreme_buckets_and_skips_only_all_zero() {
    // MAX + MAX + 2 panics in debug and wraps to zero in release.
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 9223372036854775807, "output": 9223372036854775807, "input_cache_read": 2, "input_cache_creation": 0}, "message_id": "msg-extreme"}}}
{"timestamp": 1770983420.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 0, "output": 0, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-zero"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, i64::MAX);
    assert_eq!(messages[0].tokens.output, i64::MAX);
    assert_eq!(messages[0].tokens.cache_read, 2);
    assert_eq!(messages[0].tokens.cache_write, 0);
}

#[test]
fn test_parse_kimi_non_positive_timestamps_fall_back_to_mtime() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": -1.5, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 10, "output": 1, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-negative"}}}
{"timestamp": 0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 20, "output": 2, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-zero"}}}
{"timestamp": 1770983426.420942, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 30, "output": 3, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-valid"}}}"#;
    let file = create_test_file(content);
    let mtime = file_modified_timestamp_ms(file.path());

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 3);
    // -1.5s would otherwise become -1500ms and bucket into 1969-12-31 (UTC;
    // the exact pre-epoch day depends on the local zone, the mis-dating
    // does not).
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].timestamp, mtime);
    assert_eq!(messages[1].tokens.input, 20);
    assert_eq!(messages[1].timestamp, mtime);
    assert_eq!(messages[2].tokens.input, 30);
    assert_eq!(messages[2].timestamp, 1770983426420);
}

#[test]
fn test_parse_kimi_mtime_fallback_does_not_outrank_a_real_timestamp() {
    // Same message_id, same totals, second line's timestamp unusable. The
    // fallback lands on mtime, which is newer than every real timestamp in
    // a live session file, so an untied comparison would let the corrupt
    // line's anchor replace the good one.
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983426.420942, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 10, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-dup"}}}
{"timestamp": -1, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 10, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-dup"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, 1770983426420);
}

#[test]
fn test_parse_kimi_real_timestamp_still_wins_a_tie_over_mtime_fallback() {
    // Mirror of the above with the corrupt line first, so the good anchor
    // arrives as the candidate rather than the incumbent.
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": -1, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 10, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-dup"}}}
{"timestamp": 1770983426.420942, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 10, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-dup"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, 1770983426420);
}

#[test]
fn test_parse_kimi_malformed_lines() {
    let content = r#"{"type": "metadata", "protocol_version": "1.3"}
not valid json at all
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 100, "output": 200, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-1"}}}"#;
    let file = create_test_file(content);

    let messages = parse_kimi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 100);
}
