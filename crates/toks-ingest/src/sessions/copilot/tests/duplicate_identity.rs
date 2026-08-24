use super::*;

#[test]
fn test_parse_copilot_merges_duplicate_spans_monotonically() {
    let root = r#"{"type":"span","traceId":"trace-merge","spanId":"invoke-root","name":"invoke_agent","startTime":[1775934259,0],"endTime":[1775934269,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.response.model":"gpt-5.4-mini","gen_ai.agent.id":"root-agent","gen_ai.usage.input_tokens":999,"gen_ai.usage.output_tokens":999}}"#;
    let first = r#"{"type":"span","traceId":"trace-merge","spanId":"span-merge","parentSpanId":"invoke-root","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934263,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-merge","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":20,"gen_ai.usage.cache_read.input_tokens":30,"gen_ai.usage.cache_write.input_tokens":40,"gen_ai.usage.reasoning_tokens":50}}"#;
    let second = r#"{"type":"span","traceId":"trace-merge","spanId":"span-merge","parentSpanId":"invoke-root","name":"chat gpt-5.4-mini","startTime":[1775934262,0],"endTime":[1775934268,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-merge","gen_ai.agent.id":"agent-merge","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":10,"gen_ai.usage.cache_read.input_tokens":40,"gen_ai.usage.cache_write.input_tokens":20,"gen_ai.usage.reasoning_tokens":60}}"#;
    let forward_file = create_test_file(&format!("{root}\n{first}\n{second}\n"));
    let reverse_file = create_test_file(&format!("{root}\n{second}\n{first}\n"));

    let forward = parse_copilot_file(forward_file.path());
    let reverse = parse_copilot_file(reverse_file.path());

    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 1);
    let message = &forward[0];
    assert_eq!(message.tokens.input, 160);
    assert_eq!(message.tokens.output, 20);
    assert_eq!(message.tokens.cache_read, 40);
    assert_eq!(message.tokens.cache_write, 40);
    assert_eq!(message.tokens.reasoning, 60);
    assert_eq!(message.timestamp, 1_775_934_260_000);
    assert_eq!(message.duration_ms, Some(8_000));
    assert_eq!(message.agent.as_deref(), Some("agent-merge"));
    assert_eq!(message.dedup_key.as_deref(), Some("trace-merge:span-merge"));
}

#[test]
fn test_parse_copilot_duplicate_uses_end_only_update_for_interval() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-end-update","spanId":"span-end-update","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-end-update","spanId":"span-end-update","name":"chat gpt-5.4-mini","endTime":[1775934265,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, 1_775_934_260_000);
    assert_eq!(messages[0].duration_ms, Some(5_000));
}

#[test]
fn test_parse_copilot_duplicate_end_only_updates_do_not_invent_duration() {
    let first = r#"{"type":"span","traceId":"trace-end-only","spanId":"span-end-only","name":"chat gpt-5.4-mini","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#;
    let second = r#"{"type":"span","traceId":"trace-end-only","spanId":"span-end-only","name":"chat gpt-5.4-mini","endTime":[1775934265,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#;
    let forward_file = create_test_file(&format!("{first}\n{second}\n"));
    let reverse_file = create_test_file(&format!("{second}\n{first}\n"));

    let forward = parse_copilot_file(forward_file.path());
    let reverse = parse_copilot_file(reverse_file.path());

    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 1);
    assert_eq!(forward[0].timestamp, 1_775_934_261_000);
    assert_eq!(forward[0].duration_ms, None);
}

#[test]
fn test_parse_copilot_duplicate_fallback_timestamp_is_not_interval_start() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-fallback-time","spanId":"span-fallback-time","name":"chat gpt-5.4-mini","attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-fallback-time","spanId":"span-fallback-time","name":"chat gpt-5.4-mini","endTime":[4102444800,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].duration_ms, None);
}

#[test]
fn test_parse_copilot_duplicate_duration_only_fallback_is_not_interval_start() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-fallback-duration","spanId":"span-fallback-duration","name":"chat gpt-5.4-mini","duration":[1,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-fallback-duration","spanId":"span-fallback-duration","name":"chat gpt-5.4-mini","endTime":[4102444800,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].duration_ms, Some(1_000));
}

#[test]
fn test_parse_copilot_duplicate_keeps_larger_duration_only_update() {
    let interval = r#"{"type":"span","traceId":"trace-duration-update","spanId":"span-duration-update","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#;
    let duration_only = r#"{"type":"span","traceId":"trace-duration-update","spanId":"span-duration-update","name":"chat gpt-5.4-mini","duration":[5,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#;
    let forward_file = create_test_file(&format!("{interval}\n{duration_only}\n"));
    let reverse_file = create_test_file(&format!("{duration_only}\n{interval}\n"));

    let forward = parse_copilot_file(forward_file.path());
    let reverse = parse_copilot_file(reverse_file.path());

    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 1);
    assert_eq!(forward[0].timestamp, 1_775_934_260_000);
    assert_eq!(forward[0].duration_ms, Some(5_000));
}

#[test]
fn test_parse_copilot_duplicate_direct_agents_are_order_independent() {
    let first = r#"{"type":"span","traceId":"trace-agent-merge","spanId":"span-agent-merge","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.agent.id":"agent-z","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#;
    let second = r#"{"type":"span","traceId":"trace-agent-merge","spanId":"span-agent-merge","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.agent.id":"agent-a","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#;
    let forward_file = create_test_file(&format!("{first}\n{second}\n"));
    let reverse_file = create_test_file(&format!("{second}\n{first}\n"));

    let forward = parse_copilot_file(forward_file.path());
    let reverse = parse_copilot_file(reverse_file.path());

    assert_eq!(forward, reverse);
    assert_eq!(forward.len(), 1);
    assert_eq!(forward[0].agent.as_deref(), Some("agent-a"));
}

#[test]
fn test_parse_copilot_duplicate_normalizes_merged_cache_read() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-cache-merge","spanId":"span-cache-merge","name":"chat gpt-5.4-mini","attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":1000,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-cache-merge","spanId":"span-cache-merge","name":"chat gpt-5.4-mini","attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":1000,"gen_ai.usage.output_tokens":10,"gen_ai.usage.cache_read.input_tokens":500}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 500);
    assert_eq!(messages[0].tokens.cache_read, 500);
}

#[test]
fn test_parse_copilot_duplicate_keeps_primary_identity() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-identity","spanId":"span-identity","name":"chat claude-sonnet-4.5","startTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"claude-sonnet-4.5","gen_ai.response.model":"claude-sonnet-4.5","gen_ai.conversation.id":"primary-session","gen_ai.usage.input_tokens":10,"gen_ai.usage.output_tokens":2}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-identity","spanId":"span-identity","name":"chat gpt-5.4","startTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4","gen_ai.response.model":"gpt-5.4","gen_ai.conversation.id":"duplicate-session","gen_ai.usage.input_tokens":20,"gen_ai.usage.output_tokens":3}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4.5");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].session_id, "primary-session");
}

#[test]
fn test_parse_copilot_keeps_different_duplicate_keys() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-keys","spanId":"span-a","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":10,"gen_ai.usage.output_tokens":2}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-keys","spanId":"span-b","name":"chat gpt-5.4-mini","startTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":20,"gen_ai.usage.output_tokens":3}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 2);
    assert!(messages.iter().any(|message| {
        message.dedup_key.as_deref() == Some("trace-keys:span-a") && message.tokens.input == 10
    }));
    assert!(messages.iter().any(|message| {
        message.dedup_key.as_deref() == Some("trace-keys:span-b") && message.tokens.input == 20
    }));
}

#[test]
fn test_parse_copilot_priority_filtered_duplicate_does_not_merge() {
    let content = concat!(
        r#"{"type":"span","traceId":"trace-priority","spanId":"span-priority","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":11,"gen_ai.usage.output_tokens":2}}"#,
        "\n",
        r#"{"type":"span","traceId":"trace-priority","spanId":"span-priority","name":"invoke_agent gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934269,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":999,"gen_ai.usage.output_tokens":999}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("trace-priority:span-priority")
    );
    assert_eq!(messages[0].tokens.input, 11);
    assert_eq!(messages[0].tokens.output, 2);
}

#[test]
fn test_merge_copilot_duplicate_recovers_agent_and_keeps_primary_identity() {
    let primary = CopilotUsageCandidate {
        source: CopilotUsageSource::ChatSpan,
        trace_id: Some("trace-merge-helper".to_string()),
        response_id: None,
        model: "primary-model".to_string(),
        provider_id: "primary-provider".to_string(),
        session_id: "primary-session".to_string(),
        timestamp_ms: 100,
        duration_ms: Some(20),
        start_timestamp_ms: Some(100),
        end_timestamp_ms: Some(120),
        inclusive_input_tokens: 40,
        tokens: TokenBreakdown {
            input: 10,
            output: 2,
            cache_read: 30,
            cache_write: 4,
            reasoning: 5,
        },
        dedup_key: "same-key".to_string(),
        agent: Some("fallback-agent".to_string()),
        agent_is_direct: false,
    };
    let duplicate = CopilotUsageCandidate {
        source: CopilotUsageSource::AgentSummarySpan,
        trace_id: Some("trace-duplicate".to_string()),
        response_id: Some("response-duplicate".to_string()),
        model: "duplicate-model".to_string(),
        provider_id: "duplicate-provider".to_string(),
        session_id: "duplicate-session".to_string(),
        timestamp_ms: 90,
        duration_ms: Some(30),
        start_timestamp_ms: Some(90),
        end_timestamp_ms: Some(120),
        inclusive_input_tokens: 60,
        tokens: TokenBreakdown {
            input: 20,
            output: 1,
            cache_read: 40,
            cache_write: 8,
            reasoning: 6,
        },
        dedup_key: "same-key".to_string(),
        agent: Some("recovered-agent".to_string()),
        agent_is_direct: true,
    };

    let merged = merge_duplicate_candidates(vec![primary, duplicate]);

    assert_eq!(merged.len(), 1);
    let candidate = &merged[0];
    assert!(candidate.source == CopilotUsageSource::ChatSpan);
    assert_eq!(candidate.trace_id.as_deref(), Some("trace-merge-helper"));
    assert_eq!(candidate.model, "primary-model");
    assert_eq!(candidate.provider_id, "primary-provider");
    assert_eq!(candidate.session_id, "primary-session");
    assert_eq!(candidate.timestamp_ms, 90);
    assert_eq!(candidate.duration_ms, Some(30));
    assert_eq!(candidate.tokens.input, 20);
    assert_eq!(candidate.tokens.output, 2);
    assert_eq!(candidate.tokens.cache_read, 40);
    assert_eq!(candidate.tokens.cache_write, 8);
    assert_eq!(candidate.tokens.reasoning, 6);
    assert_eq!(candidate.agent.as_deref(), Some("recovered-agent"));
}

#[test]
fn adversarial_zero_span_identity_spans_are_not_collapsed() {
    // W3C/OTel "invalid" ids are all-zeros. Two UNRELATED chat spans that
    // both carry the invalid all-zero traceId/spanId must not be merged
    // into one message: they are distinct requests whose exporter simply
    // had no recording span context. Expected: 2 messages, totals summed.
    let content = concat!(
        r#"{"type":"span","traceId":"00000000000000000000000000000000","spanId":"0000000000000000","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-zero","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","traceId":"00000000000000000000000000000000","spanId":"0000000000000000","name":"chat claude-sonnet-4.5","startTime":[1775934300,0],"endTime":[1775934301,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"claude-sonnet-4.5","gen_ai.conversation.id":"conv-zero","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|m| m.tokens.output).sum();
    assert_eq!(
        (messages.len(), total_input, total_output),
        (2, 300, 30),
        "unrelated zero-id spans were collapsed: {:?}",
        messages
            .iter()
            .map(|m| (m.model_id.clone(), m.tokens.input))
            .collect::<Vec<_>>()
    );
}

#[test]
fn zero_top_level_ids_fall_through_to_valid_span_context_ids() {
    // A zero top-level sentinel must not mask a valid nested spanContext
    // identity: duplicate snapshots of the SAME span, identified only via
    // spanContext, must still merge instead of falling back to the
    // line-index key and double counting.
    let content = concat!(
        r#"{"type":"span","traceId":"00000000000000000000000000000000","spanId":"0000000000000000","spanContext":{"traceId":"aaaabbbbccccddddaaaabbbbccccdddd","spanId":"1122334455667788"},"name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-ctx","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","traceId":"00000000000000000000000000000000","spanId":"0000000000000000","spanContext":{"traceId":"aaaabbbbccccddddaaaabbbbccccdddd","spanId":"1122334455667788"},"name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934262,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-ctx","gen_ai.usage.input_tokens":150,"gen_ai.usage.output_tokens":12}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(
        messages.len(),
        1,
        "duplicate snapshots with valid spanContext ids behind zero top-level ids must merge: keys {:?}",
        messages
            .iter()
            .map(|m| m.dedup_key.clone())
            .collect::<Vec<_>>()
    );
    assert_eq!(messages[0].tokens.input, 150);
}

#[test]
fn adversarial_spanid_only_duplicates_do_merge() {
    // Duplicate exporter snapshots of the SAME span (same spanId) that lack
    // a traceId. Per the #939 intent these should merge into one message,
    // but the fallback dedup key previously ignored span_id and appended
    // the line index, so they stayed distinct -> double count.
    let content = concat!(
        r#"{"type":"span","spanId":"span-dup","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-dup","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}"#,
        "\n",
        r#"{"type":"span","spanId":"span-dup","name":"chat gpt-5.4-mini","startTime":[1775934260,0],"endTime":[1775934262,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-dup","gen_ai.usage.input_tokens":150,"gen_ai.usage.output_tokens":12}}"#,
    );
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(
        messages.len(),
        1,
        "spanId-only duplicate snapshots were not merged: keys {:?}",
        messages
            .iter()
            .map(|m| m.dedup_key.clone())
            .collect::<Vec<_>>()
    );
}
