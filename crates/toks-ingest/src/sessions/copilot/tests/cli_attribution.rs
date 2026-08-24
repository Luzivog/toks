use super::*;

#[test]
fn test_parse_copilot_cli_underscore_cache_attributes() {
    // Copilot CLI OTEL emits cache fields with underscores instead of dots:
    // gen_ai.usage.cache_read_input_tokens / gen_ai.usage.cache_creation_input_tokens
    let content = r#"{"type":"span","traceId":"trace-cli","spanId":"span-cli","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"resource":{"attributes":{"service.name":"github-copilot","service.version":"1.0.62"}},"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.usage.input_tokens":21884,"gen_ai.usage.output_tokens":80,"gen_ai.usage.cache_creation_input_tokens":21881}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.cache_write, 21881);
    assert_eq!(messages[0].tokens.cache_read, 0);
}

#[test]
fn test_parse_copilot_cli_underscore_cache_read_and_creation() {
    let content = r#"{"type":"span","traceId":"trace-cli2","spanId":"span-cli2","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"resource":{"attributes":{"service.name":"github-copilot"}},"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.usage.input_tokens":23000,"gen_ai.usage.output_tokens":120,"gen_ai.usage.cache_read_input_tokens":21881,"gen_ai.usage.cache_creation_input_tokens":1397}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.cache_read, 21881);
    assert_eq!(messages[0].tokens.cache_write, 1397);
}

#[test]
fn test_parse_copilot_cli_sets_agent_from_invoke_agent_span() {
    // invoke_agent and chat spans share a traceId; gen_ai.agent.id from
    // invoke_agent should propagate to chat messages via TraceContext so
    // the Agents tab is populated for Copilot CLI sessions.
    let content = r#"{"type":"span","traceId":"trace-agent","spanId":"invoke-1","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.conversation.id":"conv-agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.default","gen_ai.agent.version":"1.0.62"}}
{"type":"span","traceId":"trace-agent","spanId":"chat-1","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.conversation.id":"conv-agent","gen_ai.usage.input_tokens":5000,"gen_ai.usage.output_tokens":100}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent.as_deref(), Some("github.copilot.default"));
}

#[test]
fn test_parse_copilot_cli_trims_whitespace_agent_id() {
    // The invoke_agent span carries a gen_ai.agent.id padded with
    // surrounding whitespace. first_non_empty_attr must store the TRIMMED
    // value so the agent id matches the same normalization branch as a
    // clean " github.copilot.default" id (without trimming, the stored
    // agent would be " github.copilot.default " and bypass normalization).
    let content = r#"{"type":"span","traceId":"trace-ws","spanId":"invoke-ws","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.conversation.id":"conv-ws","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"  github.copilot.default  "}}
{"type":"span","traceId":"trace-ws","spanId":"chat-ws","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.conversation.id":"conv-ws","gen_ai.usage.input_tokens":5000,"gen_ai.usage.output_tokens":100}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent.as_deref(), Some("github.copilot.default"));
}

#[test]
fn test_parse_copilot_cli_per_record_agent_id_wins_over_trace_agent() {
    // A trace's invoke_agent span names the default agent, but a later chat
    // record carries its OWN gen_ai.agent.id for a sub-agent. Per-record
    // attribution must win so the sub-agent's tokens are not mis-attributed
    // to the trace's first (default) agent.
    let content = r#"{"type":"span","traceId":"trace-sub","spanId":"invoke-sub","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.conversation.id":"conv-sub","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.default"}}
{"type":"span","traceId":"trace-sub","spanId":"chat-sub","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.conversation.id":"conv-sub","gen_ai.agent.id":"github.copilot.reviewer","gen_ai.usage.input_tokens":5000,"gen_ai.usage.output_tokens":100}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].agent.as_deref(),
        Some("github.copilot.reviewer")
    );
}

#[test]
fn test_parse_copilot_cli_trace_agent_prefers_invoke_agent_over_child_span() {
    // OTel export order is not guaranteed. A child chat span that carries
    // its own gen_ai.agent.id (a sub-agent turn) can be exported BEFORE the
    // parent invoke_agent span. The trace-level fallback agent must still
    // resolve to the invoke_agent span's default rather than whichever agent
    // id appears first in the trace, so a later agentless turn inherits the
    // trace default instead of the sub-agent that happened to export first.
    let content = r#"{"type":"span","traceId":"trace-order","spanId":"chat-sub","name":"chat claude-sonnet-4.6","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.response.id":"resp-sub","gen_ai.agent.id":"github.copilot.reviewer","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}
{"type":"span","traceId":"trace-order","spanId":"invoke-1","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.default"}}
{"type":"span","traceId":"trace-order","spanId":"chat-plain","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-plain","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    // The child sub-agent turn keeps its own agent id (per-record wins).
    let sub = messages
        .iter()
        .find(|message| message.model_id == "claude-sonnet-4.6")
        .unwrap();
    assert_eq!(sub.agent.as_deref(), Some("github.copilot.reviewer"));
    // The agentless turn inherits the invoke_agent default, not the
    // sub-agent that happened to export first.
    let plain = messages
        .iter()
        .find(|message| message.model_id == "gpt-5.4-mini")
        .unwrap();
    assert_eq!(plain.agent.as_deref(), Some("github.copilot.default"));
}

#[test]
fn test_parse_copilot_cli_trace_agent_prefers_root_invoke_agent_over_nested() {
    // A trace can contain several invoke_agent spans: the top-level agent
    // invocation plus a NESTED invoke_agent when the main agent launches a
    // task/sub-agent via a tool call. The nested invoke's parent chain runs
    // execute_tool -> root invoke_agent, so it must NOT become the trace
    // fallback. The nested invoke and its sub-agent chat are exported BEFORE
    // the root invoke_agent (OTel export order is not guaranteed), which is
    // exactly the case a first-invoke-wins lock would mis-attribute.
    let content = r#"{"type":"span","traceId":"trace-nested","spanId":"invoke-sub","parentSpanId":"tool-task","name":"invoke_agent","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.subagent"}}
{"type":"span","traceId":"trace-nested","spanId":"chat-sub","parentSpanId":"invoke-sub","name":"chat claude-sonnet-4.6","endTime":[1775934262,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.response.id":"resp-sub","gen_ai.agent.id":"github.copilot.subagent","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}
{"type":"span","traceId":"trace-nested","spanId":"tool-task","parentSpanId":"invoke-root","name":"execute_tool task","endTime":[1775934263,0],"attributes":{"gen_ai.operation.name":"execute_tool","gen_ai.tool.name":"task"}}
{"type":"span","traceId":"trace-nested","spanId":"invoke-root","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.default"}}
{"type":"span","traceId":"trace-nested","spanId":"chat-plain","parentSpanId":"invoke-root","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-plain","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    // The sub-agent chat keeps its own agent id (per-record wins).
    let sub = messages
        .iter()
        .find(|message| message.model_id == "claude-sonnet-4.6")
        .unwrap();
    assert_eq!(sub.agent.as_deref(), Some("github.copilot.subagent"));
    // The agentless turn inherits the ROOT invoke_agent default, not the
    // nested sub-agent invoke that exported first.
    let plain = messages
        .iter()
        .find(|message| message.model_id == "gpt-5.4-mini")
        .unwrap();
    assert_eq!(plain.agent.as_deref(), Some("github.copilot.default"));
}

#[test]
fn test_parse_copilot_cli_trace_agent_single_invoke_agent_unchanged() {
    // The common single-invoke_agent trace (no nesting) is unaffected by the
    // root-preference logic: the one invoke_agent span is trivially the root,
    // so its agent id still propagates to an agentless chat turn.
    let content = r#"{"type":"span","traceId":"trace-single","spanId":"invoke-1","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.default"}}
{"type":"span","traceId":"trace-single","spanId":"chat-1","parentSpanId":"invoke-1","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-single","gen_ai.usage.input_tokens":50,"gen_ai.usage.output_tokens":8}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    let plain = messages
        .iter()
        .find(|message| message.model_id == "gpt-5.4-mini")
        .unwrap();
    assert_eq!(plain.agent.as_deref(), Some("github.copilot.default"));
}

#[test]
fn test_parse_copilot_cli_trace_agent_links_through_attribute_less_intermediary() {
    let content = r#"{"type":"span","traceId":"trace-attrless","spanId":"invoke-sub","parentSpanId":"tool-task","name":"invoke_agent","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.subagent"}}
{"type":"span","traceId":"trace-attrless","spanId":"chat-sub","parentSpanId":"invoke-sub","name":"chat claude-sonnet-4.6","endTime":[1775934262,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.response.id":"resp-sub","gen_ai.agent.id":"github.copilot.subagent","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":10}}
{"type":"span","traceId":"trace-attrless","spanId":"tool-task","parentSpanId":"invoke-root","name":"execute_tool task"}
{"type":"span","traceId":"trace-attrless","spanId":"invoke-root","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.default"}}
{"type":"span","traceId":"trace-attrless","spanId":"chat-plain","parentSpanId":"invoke-root","name":"chat gpt-5.4-mini","endTime":[1775934264,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-plain","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    let sub = messages
        .iter()
        .find(|message| message.model_id == "claude-sonnet-4.6")
        .unwrap();
    assert_eq!(sub.agent.as_deref(), Some("github.copilot.subagent"));

    let plain = messages
        .iter()
        .find(|message| message.model_id == "gpt-5.4-mini")
        .unwrap();
    assert_eq!(plain.agent.as_deref(), Some("github.copilot.default"));
}

#[test]
fn test_parse_copilot_cli_trace_agent_scopes_reused_span_ids_per_trace() {
    let content = r#"{"type":"span","traceId":"trace-scope-a","spanId":"invoke-nested-a","parentSpanId":"tool-a","name":"invoke_agent","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.subagent-a"}}
{"type":"span","traceId":"trace-scope-a","spanId":"tool-a","parentSpanId":"invoke-shared","name":"execute_tool task","endTime":[1775934262,0],"attributes":{"gen_ai.operation.name":"execute_tool","gen_ai.tool.name":"task"}}
{"type":"span","traceId":"trace-scope-a","spanId":"invoke-shared","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.root-a"}}
{"type":"span","traceId":"trace-scope-a","spanId":"chat-a","parentSpanId":"invoke-shared","name":"chat gpt-5.4-mini","endTime":[1775934264,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-a","gen_ai.usage.input_tokens":200,"gen_ai.usage.output_tokens":20}}
{"type":"span","traceId":"trace-scope-b","spanId":"invoke-outer-b","name":"invoke_agent","endTime":[1775934260,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.root-b"}}
{"type":"span","traceId":"trace-scope-b","spanId":"invoke-shared","parentSpanId":"invoke-outer-b","name":"invoke_agent","endTime":[1775934261,0],"attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.agent.id":"github.copilot.subagent-b"}}
{"type":"span","traceId":"trace-scope-b","spanId":"chat-b","parentSpanId":"invoke-outer-b","name":"chat gpt-5.4-mini","endTime":[1775934264,0],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.response.id":"resp-b","gen_ai.usage.input_tokens":300,"gen_ai.usage.output_tokens":30}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    let chat_a = messages
        .iter()
        .find(|message| message.dedup_key.as_deref() == Some("trace-scope-a:chat-a"))
        .unwrap();
    assert_eq!(chat_a.agent.as_deref(), Some("github.copilot.root-a"));

    let chat_b = messages
        .iter()
        .find(|message| message.dedup_key.as_deref() == Some("trace-scope-b:chat-b"))
        .unwrap();
    assert_eq!(chat_b.agent.as_deref(), Some("github.copilot.root-b"));
}

#[test]
fn test_parse_copilot_cli_underscore_cache_write_attribute() {
    // Copilot CLI may emit the cache-write bucket with the fully
    // underscored key gen_ai.usage.cache_write_input_tokens (a sibling of
    // the documented cache_read_input_tokens variant). It must map to the
    // cache_write token bucket.
    let content = r#"{"type":"span","traceId":"trace-cw","spanId":"span-cw","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"resource":{"attributes":{"service.name":"github-copilot"}},"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.usage.input_tokens":21884,"gen_ai.usage.output_tokens":80,"gen_ai.usage.cache_write_input_tokens":21881}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.cache_write, 21881);
    assert_eq!(messages[0].tokens.cache_read, 0);
}

#[test]
fn test_parse_copilot_cli_no_agent_when_invoke_agent_absent() {
    let content = r#"{"type":"span","traceId":"trace-noagent","spanId":"chat-1","name":"chat claude-sonnet-4.6","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.provider.name":"github","gen_ai.request.model":"claude-sonnet-4.6","gen_ai.response.model":"claude-sonnet-4.6","gen_ai.conversation.id":"conv-noagent","gen_ai.usage.input_tokens":1000,"gen_ai.usage.output_tokens":50}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent, None);
}
