use super::*;

#[test]
fn keeps_parsing_updates_after_an_undecodable_line() {
    let mut fixture = Vec::new();
    fixture.extend_from_slice(usage_line("turn-1", 1_700_000_001_000, 10, 1).as_bytes());
    fixture.push(b'\n');
    // A lone 0xff can never appear in valid UTF-8, so `BufRead::lines()`
    // reports this line as `InvalidData`.
    fixture.extend_from_slice(b"{\"garbage\":\"\xff\xfe\"}\n");
    for index in 2..=100i64 {
        fixture.extend_from_slice(
            usage_line(
                &format!("turn-{index}"),
                1_700_000_001_000 + index * 1000,
                10,
                1,
            )
            .as_bytes(),
        );
        fixture.push(b'\n');
    }

    let (_temp, path) = write_fixture(&fixture, None, None);
    let messages = parse_grok_updates_file(&path);

    assert_eq!(messages.len(), 100);
    assert_eq!(messages.last().unwrap().timestamp, 1_700_000_101_000);
}

#[test]
fn parses_first_update_of_a_bom_prefixed_file() {
    let mut fixture = Vec::new();
    fixture.extend_from_slice("\u{feff}".as_bytes());
    fixture.extend_from_slice(usage_line("turn-1", 1_700_000_001_000, 10, 1).as_bytes());
    fixture.push(b'\n');
    fixture.extend_from_slice(usage_line("turn-2", 1_700_000_002_000, 20, 2).as_bytes());
    fixture.push(b'\n');

    let (_temp, path) = write_fixture(&fixture, None, None);
    let messages = parse_grok_updates_file(&path);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].timestamp, 1_700_000_001_000);
    assert_eq!(messages[0].tokens.input, 10);
}

#[test]
fn keeps_repeated_event_ids_in_distinct_dedup_keys() {
    let (_temp, path) = write_fixture(
        format!(
            "{}\n{}\n",
            usage_line("turn-1", 1_700_000_001_000, 10, 1),
            usage_line("turn-1", 1_700_000_002_000, 20, 2),
        ),
        None,
        None,
    );

    let messages = parse_grok_updates_file(&path);

    assert_eq!(messages.len(), 2);
    assert_ne!(messages[0].dedup_key, messages[1].dedup_key);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("grok:session-1:usage:0:turn-1")
    );
    assert_eq!(
        messages[1].dedup_key.as_deref(),
        Some("grok:session-1:usage:1:turn-1")
    );
}

#[test]
fn prefers_authoritative_usage_breakdown_when_available() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-4.5"}},"_meta":{"agentTimestampMs":1700000001000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":1200,"agentTimestampMs":1700000002000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":1000,"outputTokens":100,"reasoningTokens":20,"cachedReadTokens":400,"totalTokens":1100,"modelUsage":{"grok-4.5-build":{"inputTokens":1000,"outputTokens":100,"reasoningTokens":20,"cachedReadTokens":400,"totalTokens":1100}}}},"_meta":{"eventId":"turn-1","agentTimestampMs":1700000003000}}}"#,
        None,
        None,
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "grok-4.5");
    assert_eq!(messages[0].tokens.input, 600);
    assert_eq!(messages[0].tokens.output, 80);
    assert_eq!(messages[0].tokens.cache_read, 400);
    assert_eq!(messages[0].tokens.reasoning, 20);
    assert_eq!(messages[0].timestamp, 1700000003000);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("grok:session-1:usage:0:turn-1")
    );
}

#[test]
fn parses_inclusive_usage_buckets_without_total_tokens() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"turn_completed","usage":{"inputTokens":100,"outputTokens":25,"reasoningTokens":5,"cachedReadTokens":60}} ,"_meta":{"agentTimestampMs":1700000003000}}}"#,
        None,
        None,
    );

    let messages = parse_grok_updates_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 40);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 60);
    assert_eq!(messages[0].tokens.reasoning, 5);
    assert_eq!(messages[0].tokens.total(), 125);
}

#[test]
fn parses_grok_total_token_deltas_by_turn() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"available_commands_update"},"_meta":{"totalTokens":100,"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-composer-2.5-fast"}},"_meta":{"agentTimestampMs":1700000001000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_thought_chunk"},"_meta":{"totalTokens":250,"agentTimestampMs":1700000002000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":300,"agentTimestampMs":1700000003000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-composer-2.5-fast"}},"_meta":{"agentTimestampMs":1700000004000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":450,"agentTimestampMs":1700000005000}}}"#,
        Some(
            r#"{"current_model_id":"grok-composer-2.5-fast","updated_at":"2023-11-14T22:13:20Z"}"#,
        ),
        None,
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].client, "grok");
    assert_eq!(messages[0].model_id, "grok-composer-2.5-fast");
    assert_eq!(messages[0].provider_id, "xai");
    assert_eq!(messages[0].session_id, "session-1");
    assert_eq!(messages[0].tokens.input, 200);
    assert_eq!(messages[0].tokens.output, 0);
    assert_eq!(messages[0].timestamp, 1700000003000);
    assert_eq!(messages[0].workspace_key.as_deref(), Some("/tmp/project"));
    assert_eq!(messages[0].workspace_label.as_deref(), Some("project"));
    assert_eq!(messages[1].tokens.input, 150);
    assert_eq!(messages[1].timestamp, 1700000005000);
}

#[test]
fn uses_summary_model_when_update_model_is_missing() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk"},"_meta":{"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":220,"agentTimestampMs":1700000001000}}}"#,
        Some(
            r#"{"current_model_id":"grok-composer-2.5-fast","updated_at":"2023-11-14T22:13:20Z"}"#,
        ),
        None,
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "grok-composer-2.5-fast");
    assert_eq!(messages[0].tokens.input, 220);
}

#[test]
fn ignores_repeated_and_decreasing_total_tokens() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"available_commands_update"},"_meta":{"totalTokens":100,"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-composer-2.5-fast"}},"_meta":{"agentTimestampMs":1700000001000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":150,"agentTimestampMs":1700000002000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":150,"agentTimestampMs":1700000003000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":120,"agentTimestampMs":1700000004000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":200,"agentTimestampMs":1700000005000}}}"#,
        None,
        None,
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].timestamp, 1700000005000);
}

#[test]
fn preserves_total_tokens_without_model_metadata() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"available_commands_update"},"_meta":{"totalTokens":120,"agentTimestampMs":1700000000000}}}"#,
        None,
        None,
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, UNKNOWN_MODEL);
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].timestamp, 1700000000000);
}

#[test]
fn creates_unknown_model_turn_without_model_metadata() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"available_commands_update"},"_meta":{"totalTokens":100,"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":250,"agentTimestampMs":1700000002000}}}"#,
        None,
        None,
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, UNKNOWN_MODEL);
    assert_eq!(messages[0].tokens.input, 150);
    assert_eq!(messages[0].timestamp, 1700000002000);
}
