use super::*;

#[test]
fn test_parse_copilot_vscode_chat_span_without_type() {
    let content = r#"{"resource":{"attributes":{"service.name":"copilot-chat"}},"instrumentationScope":{"name":"copilot-chat","version":"0.44.0"},"traceId":"trace-vscode","spanId":"span-vscode","name":"chat claude-sonnet-4.5","kind":2,"endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.5","gen_ai.response.model":"claude-sonnet-4.5","gen_ai.conversation.id":"conv-vscode","gen_ai.usage.input_tokens":1000,"gen_ai.usage.output_tokens":50,"gen_ai.usage.cache_read.input_tokens":200,"gen_ai.usage.cache_creation.input_tokens":75,"gen_ai.usage.reasoning_tokens":12}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4.5");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].session_id, "conv-vscode");
    assert_eq!(messages[0].tokens.input, 800);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 200);
    assert_eq!(messages[0].tokens.cache_write, 75);
    assert_eq!(messages[0].tokens.reasoning, 12);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("trace-vscode:span-vscode")
    );
}

#[test]
fn test_parse_copilot_vscode_inference_log_when_span_is_unavailable() {
    let content = r#"{"hrTime":[1775934264,967317833],"spanContext":{"traceId":"trace-log","spanId":"span-log","traceFlags":1},"instrumentationScope":{"name":"copilot-chat","version":"0.44.0"},"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"response-log","gen_ai.usage.input_tokens":42,"gen_ai.usage.output_tokens":7},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.4-mini");
    assert_eq!(messages[0].session_id, "response-log");
    assert_eq!(messages[0].tokens.input, 42);
    assert_eq!(messages[0].tokens.output, 7);
    assert_eq!(messages[0].timestamp, 1_775_934_264_967);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("log:trace-log:span-log")
    );
}

#[test]
fn test_parse_copilot_prefers_chat_spans_over_agent_summary() {
    let content = r#"{"traceId":"trace-dupe","spanId":"agent-1","name":"invoke_agent GitHub Copilot Chat","endTime":[1775934270,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-dupe","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":30}}
{"traceId":"trace-dupe","spanId":"chat-1","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-dupe","gen_ai.usage.input_tokens":60,"gen_ai.usage.output_tokens":10}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("trace-dupe:chat-1"));
    assert_eq!(messages[0].tokens.input, 60);
    assert_eq!(messages[0].tokens.output, 10);
}

#[test]
fn test_parse_copilot_agent_turn_log_uses_trace_context_as_last_resort() {
    let content = r#"{"hrTime":[1775934260,0],"spanContext":{"traceId":"trace-turn","spanId":"session-log","traceFlags":1},"attributes":{"event.name":"copilot_chat.session.start","session.id":"conv-turn","gen_ai.request.model":"claude-sonnet-4.5"},"_body":"copilot_chat.session.start"}
{"hrTime":[1775934264,967317833],"spanContext":{"traceId":"trace-turn","spanId":"turn-log","traceFlags":1},"attributes":{"event.name":"copilot_chat.agent.turn","turn.index":3,"gen_ai.usage.input_tokens":120,"gen_ai.usage.output_tokens":9},"_body":"copilot_chat.agent.turn: 3"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4.5");
    assert_eq!(messages[0].session_id, "conv-turn");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 9);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("agent-turn:trace-turn:3")
    );
}

#[test]
fn test_parse_copilot_prefers_chat_span_over_agent_turn_in_same_trace() {
    let content = r#"{"type":"span","traceId":"trace-mix","spanId":"chat-mix","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-mix","gen_ai.usage.input_tokens":50,"gen_ai.usage.output_tokens":8}}
{"hrTime":[1775934265,0],"spanContext":{"traceId":"trace-mix","spanId":"turn-mix","traceFlags":1},"attributes":{"event.name":"copilot_chat.agent.turn","turn.index":1,"gen_ai.usage.input_tokens":50,"gen_ai.usage.output_tokens":8},"_body":"copilot_chat.agent.turn: 1"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("trace-mix:chat-mix"));
    assert_eq!(messages[0].tokens.input, 50);
    assert_eq!(messages[0].tokens.output, 8);
}

#[test]
fn test_parse_copilot_traceless_records_do_not_cross_suppress() {
    // Two traceless records describing distinct OTel responses must both
    // emit even when they share a coarse session attribute (here
    // gen_ai.conversation.id, which spans an entire chat). Cross-source
    // suppression must key on the per-response identifier
    // (gen_ai.response.id), not on chat-wide session attributes.
    let content = r#"{"type":"span","spanId":"chat-traceless","name":"chat gpt-5.4-mini","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-shared","gen_ai.response.id":"resp-A","gen_ai.usage.input_tokens":11,"gen_ai.usage.output_tokens":3}}
{"hrTime":[1775934262,0],"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-shared","gen_ai.response.id":"resp-B","gen_ai.usage.input_tokens":22,"gen_ai.usage.output_tokens":4},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 2);
    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|m| m.tokens.output).sum();
    assert_eq!(total_input, 33);
    assert_eq!(total_output, 7);
}

#[test]
fn test_parse_copilot_agent_turn_log_without_turn_index_uses_line_index() {
    // Two agent-turn records in the same trace with no turn.index attribute
    // must produce distinct dedup keys (no `0` sentinel collision).
    let content = r#"{"hrTime":[1775934260,0],"spanContext":{"traceId":"trace-noidx","spanId":"turn-a","traceFlags":1},"attributes":{"event.name":"copilot_chat.agent.turn","gen_ai.request.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":10,"gen_ai.usage.output_tokens":2},"_body":"copilot_chat.agent.turn"}
{"hrTime":[1775934261,0],"spanContext":{"traceId":"trace-noidx","spanId":"turn-b","traceFlags":1},"attributes":{"event.name":"copilot_chat.agent.turn","gen_ai.request.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":11,"gen_ai.usage.output_tokens":3},"_body":"copilot_chat.agent.turn"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 2);
    let mut keys: Vec<String> = messages
        .iter()
        .filter_map(|m| m.dedup_key.clone())
        .collect();
    keys.sort();
    assert_eq!(keys.len(), 2);
    assert_ne!(keys[0], keys[1], "dedup keys must be unique: {keys:?}");
    for key in &keys {
        assert!(
            key.starts_with("agent-turn:trace-noidx:idx-"),
            "expected line-index fallback shape in {key}",
        );
    }
}

#[test]
fn test_parse_copilot_inference_log_uses_time_unix_nano_timestamp() {
    let content = r#"{"timeUnixNano":1775934264967317833,"spanContext":{"traceId":"trace-nano","spanId":"span-nano","traceFlags":1},"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-nano","gen_ai.usage.input_tokens":5,"gen_ai.usage.output_tokens":2},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, 1_775_934_264_967);
}

#[test]
fn test_parse_copilot_agent_turn_log_uses_scalar_timestamp() {
    let content = r#"{"timestamp":1775934264967,"spanContext":{"traceId":"trace-ts","spanId":"turn-ts","traceFlags":1},"attributes":{"event.name":"copilot_chat.agent.turn","turn.index":2,"gen_ai.request.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":7,"gen_ai.usage.output_tokens":1},"_body":"copilot_chat.agent.turn: 2"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, 1_775_934_264_967);
}

#[test]
fn test_parse_copilot_mixed_trace_double_count_suppressed_via_response_id() {
    // Mixed-trace gap: a traceless chat span and a traced inference log
    // describe the same OTel response (same gen_ai.response.id). With no
    // shared trace_id, the response-id key is what links them; only the
    // higher-priority chat span should emit.
    let content = r#"{"type":"span","spanId":"chat-mixed","name":"chat gpt-5.4-mini","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-mixed","gen_ai.response.id":"resp-mixed","gen_ai.usage.input_tokens":40,"gen_ai.usage.output_tokens":7}}
{"hrTime":[1775934261,0],"spanContext":{"traceId":"trace-mixed-inf","spanId":"inf-mixed","traceFlags":1},"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-mixed","gen_ai.usage.input_tokens":40,"gen_ai.usage.output_tokens":7},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "conv-mixed");
    assert_eq!(messages[0].tokens.input, 40);
    assert_eq!(messages[0].tokens.output, 7);
}

#[test]
fn test_parse_copilot_traced_chat_suppresses_traceless_inference_via_response_id() {
    // Inverse of the mixed-trace gap: a traced chat span suppresses a
    // traceless inference log via shared gen_ai.response.id, even though
    // the log carries no trace_id to link it through.
    let content = r#"{"type":"span","traceId":"trace-chat-inv","spanId":"chat-inv","name":"chat gpt-5.4-mini","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-inv","gen_ai.response.id":"resp-inv","gen_ai.usage.input_tokens":33,"gen_ai.usage.output_tokens":5}}
{"hrTime":[1775934261,0],"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-inv","gen_ai.usage.input_tokens":33,"gen_ai.usage.output_tokens":5},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("trace-chat-inv:chat-inv"),
    );
    assert_eq!(messages[0].tokens.input, 33);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn test_parse_copilot_inference_log_negative_time_unix_nano_falls_back() {
    // Malformed `timeUnixNano` must not produce a negative timestamp; the
    // parser should fall through to the next available timestamp source
    // (here, the file modified time, which is non-negative).
    let content = r#"{"timeUnixNano":-1,"spanContext":{"traceId":"trace-bad","spanId":"span-bad","traceFlags":1},"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-bad","gen_ai.usage.input_tokens":5,"gen_ai.usage.output_tokens":2},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert!(
        messages[0].timestamp >= 0,
        "negative timeUnixNano should not leak into output, got {}",
        messages[0].timestamp,
    );
}

#[test]
fn test_parse_copilot_interleaved_multi_trace_suppression_is_per_trace() {
    // Two traces interleaved on the wire. Source-priority suppression must
    // be scoped per-trace; both invoke_agent records should be dropped in
    // favor of their own trace's chat span, regardless of line order.
    let content = r#"{"type":"span","traceId":"trace-A","spanId":"agent-A","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-A","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":30}}
{"type":"span","traceId":"trace-B","spanId":"chat-B","name":"chat gpt-5.4-mini","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-B","gen_ai.usage.input_tokens":50,"gen_ai.usage.output_tokens":8}}
{"type":"span","traceId":"trace-A","spanId":"chat-A","name":"chat gpt-5.4-mini","endTime":[1775934262,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-A","gen_ai.usage.input_tokens":40,"gen_ai.usage.output_tokens":6}}
{"type":"span","traceId":"trace-B","spanId":"agent-B","name":"invoke_agent","endTime":[1775934263,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.response.model":"gpt-5.4-mini","gen_ai.conversation.id":"conv-B","gen_ai.usage.input_tokens":80,"gen_ai.usage.output_tokens":20}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 2);
    let mut keys: Vec<String> = messages
        .iter()
        .filter_map(|m| m.dedup_key.clone())
        .collect();
    keys.sort();
    assert_eq!(
        keys,
        vec!["trace-A:chat-A".to_string(), "trace-B:chat-B".to_string()],
    );
}

#[test]
fn test_parse_copilot_agent_turn_log_with_top_level_trace_id() {
    // Some VS Code variants emit `traceId` at the top level rather than
    // nested inside `spanContext`. The agent-turn classifier should still
    // resolve the trace and produce a stable per-turn dedup key.
    let content = r#"{"hrTime":[1775934264,0],"traceId":"trace-toplevel","spanId":"turn-toplevel","attributes":{"event.name":"copilot_chat.agent.turn","turn.index":5,"gen_ai.request.model":"claude-sonnet-4.5","gen_ai.usage.input_tokens":15,"gen_ai.usage.output_tokens":4},"_body":"copilot_chat.agent.turn: 5"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4.5");
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("agent-turn:trace-toplevel:5"),
    );
}

#[test]
fn test_parse_copilot_traced_span_does_not_suppress_traceless_record_with_colliding_session() {
    // A traced chat span has trace_id "T-collide". A separate traceless
    // inference log uses "T-collide" as its session-fallback (gen_ai.response.id).
    // The traceless record must NOT be suppressed by the traced chat span's
    // context_key, because they are unrelated events. Both should emit.
    let content = r#"{"type":"span","traceId":"T-collide","spanId":"chat-traced","name":"chat gpt-5.4-mini","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":10,"gen_ai.usage.output_tokens":2}}
{"hrTime":[1775934261,0],"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"T-collide","gen_ai.usage.input_tokens":20,"gen_ai.usage.output_tokens":3},"_body":"GenAI inference: gpt-5.4-mini"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 2);
    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    let total_output: i64 = messages.iter().map(|m| m.tokens.output).sum();
    assert_eq!(total_input, 30);
    assert_eq!(total_output, 5);
}

#[test]
fn test_parse_copilot_trace_context_prefers_session_id_over_response_id() {
    let content = r#"{"hrTime":[1775934260,0],"spanContext":{"traceId":"trace-session-upgrade","spanId":"response-log","traceFlags":1},"attributes":{"event.name":"gen_ai.client.inference.operation.details","gen_ai.response.id":"response-scoped-id","gen_ai.request.model":"claude-sonnet-4.5"},"_body":"GenAI inference: claude-sonnet-4.5"}
{"hrTime":[1775934261,0],"spanContext":{"traceId":"trace-session-upgrade","spanId":"session-log","traceFlags":1},"attributes":{"event.name":"copilot_chat.session.start","session.id":"durable-session-id"},"_body":"copilot_chat.session.start"}
{"hrTime":[1775934264,967317833],"spanContext":{"traceId":"trace-session-upgrade","spanId":"turn-log","traceFlags":1},"attributes":{"event.name":"copilot_chat.agent.turn","turn.index":4,"gen_ai.usage.input_tokens":120,"gen_ai.usage.output_tokens":9},"_body":"copilot_chat.agent.turn: 4"}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4.5");
    assert_eq!(messages[0].session_id, "durable-session-id");
    assert_eq!(messages[0].tokens.input, 120);
    assert_eq!(messages[0].tokens.output, 9);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("agent-turn:trace-session-upgrade:4")
    );
}
