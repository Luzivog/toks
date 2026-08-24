use super::*;

#[test]
fn test_parse_copilot_chat_span() {
    let content = r#"{"type":"metric","name":"gen_ai.client.token.usage"}
{"type":"span","traceId":"trace-1","spanId":"span-1","name":"chat claude-sonnet-4","startTime":[1775934260,133000000],"endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"claude-sonnet-4","gen_ai.response.model":"claude-sonnet-4","gen_ai.conversation.id":"conv-1","gen_ai.usage.input_tokens":19452,"gen_ai.usage.output_tokens":281,"gen_ai.usage.cache_read.input_tokens":123,"gen_ai.usage.reasoning.output_tokens":128,"github.copilot.interaction_id":"interaction-1"}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "copilot");
    assert_eq!(message.model_id, "claude-sonnet-4");
    assert_eq!(message.provider_id, "anthropic");
    assert_eq!(message.session_id, "conv-1");
    assert_eq!(message.tokens.input, 19_329);
    assert_eq!(message.tokens.output, 281);
    assert_eq!(message.tokens.cache_read, 123);
    assert_eq!(message.tokens.reasoning, 128);
    assert_eq!(message.timestamp, 1_775_934_260_133);
    assert_eq!(message.duration_ms, Some(4834));
    assert_eq!(message.dedup_key.as_deref(), Some("trace-1:span-1"));
}

#[test]
fn test_parse_copilot_ignores_non_chat_spans() {
    let content = r#"{"type":"span","traceId":"trace-1","spanId":"tool-1","name":"execute_tool rg","attributes":{"gen_ai.operation.name":"execute_tool","gen_ai.tool.name":"rg"}}
{"type":"span","traceId":"trace-1","spanId":"invoke-1","name":"invoke_agent","attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.usage.input_tokens":999,"gen_ai.usage.output_tokens":111}}
{"type":"span","traceId":"trace-1","spanId":"chat-1","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":10,"gen_ai.usage.output_tokens":5}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].dedup_key.as_deref(), Some("trace-1:chat-1"));
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn test_parse_copilot_falls_back_to_trace_and_provider() {
    let content = r#"{"type":"span","traceId":"trace-fallback","spanId":"span-fallback","name":"chat custom-model","attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"custom-model","gen_ai.usage.input_tokens":"7","gen_ai.usage.output_tokens":"9"}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "github-copilot");
    assert_eq!(messages[0].session_id, "trace-fallback");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 9);
}

#[test]
fn test_parse_copilot_normalizes_only_cache_read_from_input() {
    let content = r#"{"type":"span","traceId":"trace-cache","spanId":"span-cache","name":"chat gpt-5.4","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4","gen_ai.usage.input_tokens":1000,"gen_ai.usage.output_tokens":20,"gen_ai.usage.cache_read.input_tokens":200,"gen_ai.usage.cache_write.input_tokens":50}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 800);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 200);
    assert_eq!(messages[0].tokens.cache_write, 50);
}

#[test]
fn test_parse_copilot_clamps_only_cache_read_to_input() {
    let content = r#"{"type":"span","traceId":"trace-clamp","spanId":"span-clamp","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":100,"gen_ai.usage.output_tokens":5,"gen_ai.usage.cache_read.input_tokens":90,"gen_ai.usage.cache_write.input_tokens":20}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.cache_read, 90);
    assert_eq!(messages[0].tokens.cache_write, 20);
}

#[test]
fn test_parse_copilot_keeps_cache_only_message() {
    let content = r#"{"type":"span","traceId":"trace-zero","spanId":"span-zero","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":0,"gen_ai.usage.cache_read.input_tokens":50,"gen_ai.usage.cache_write.input_tokens":20}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 0);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.cache_write, 20);
}

#[test]
fn test_parse_copilot_keeps_cache_read_when_input_is_missing() {
    let content = r#"{"type":"span","traceId":"trace-cache-read","spanId":"span-cache-read","name":"chat gpt-5.4-mini","endTime":[1775934264,967317833],"attributes":{"gen_ai.operation.name":"chat","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.cache_read.input_tokens":50}}"#;
    let file = create_test_file(content);

    let messages = parse_copilot_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 0);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.cache_write, 0);
}
