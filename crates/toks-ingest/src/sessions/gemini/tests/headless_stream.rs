use super::*;

#[test]
fn test_parse_headless_json() {
    let json = r#"{"response":"Hi","stats":{"models":{"gemini-2.5-pro":{"tokens":{"prompt":12,"candidates":34,"cached":5,"thoughts":2}}}}}"#;
    // Use a legacy prefix to satisfy the path check
    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 34);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 2);
    assert_eq!(messages[0].tokens.total(), 48);
}

#[test]
fn test_parse_headless_stream_jsonl() {
    let content = r#"{"type":"init","model":"gemini-2.5-pro","session_id":"session-1"}
{"type":"result","stats":{"input_tokens":10,"output_tokens":20}}"#;
    let mut file = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 20);
}

#[test]
fn test_parse_headless_stream_jsonl_normalizes_cached_input() {
    let content = r#"{"type":"init","model":"gemini-2.5-pro","session_id":"session-1"}
{"type":"result","stats":{"input_tokens":12,"output_tokens":20,"cached_tokens":5,"thoughts_tokens":3}}"#;
    let mut file = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 3);
    assert_eq!(messages[0].tokens.total(), 35);
}

#[test]
fn test_parse_gemini_stream_jsonl_v0391_model_stats_without_tokens_wrapper() {
    let content = r#"{"type":"init","model":"gemini-2.5-pro","session_id":"session-1"}
{"type":"result","stats":{"total_tokens":32,"input_tokens":12,"output_tokens":20,"cached":5,"input":7,"models":{"gemini-2.5-pro":{"total_tokens":32,"input_tokens":12,"output_tokens":20,"cached":5,"input":7}}}}"#;
    let mut file = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.total(), 32);
}

#[test]
fn test_parse_gemini_stream_jsonl_v0391_flat_stats_uses_net_input_alias() {
    let content = r#"{"type":"init","model":"gemini-2.5-pro","session_id":"session-1"}
{"type":"result","stats":{"total_tokens":32,"output_tokens":20,"cached":5,"input":7}}"#;
    let mut file = tempfile::Builder::new()
        .suffix(".jsonl")
        .tempfile()
        .unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.total(), 32);
}

#[test]
fn test_parse_headless_stats_tokens_wrapper_preserves_cache_inclusive_input() {
    let json =
        r#"{"stats":{"models":{"gemini-2.5-pro":{"tokens":{"input":12,"output":20,"cached":5}}}}}"#;
    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.total(), 32);
}

#[test]
fn test_parse_gemini_stream_jsonl_direct_tokens() {
    let content = r#"{"sessionId":"gemini-session-1","projectHash":"abc123","startTime":"2026-05-01T00:00:00.000Z","lastUpdated":"2026-05-01T00:01:00.000Z"}
{"id":"msg-1","timestamp":"2026-05-01T00:01:00.000Z","type":"gemini","model":"gemini-3.1-pro-preview","tokens":{"input":14918,"output":60,"cached":0,"thoughts":863,"tool":7,"total":15848}}"#;
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("session-abc.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "gemini-session-1");
    assert_eq!(messages[0].model_id, "gemini-3.1-pro-preview");
    assert_eq!(messages[0].provider_id, "google");
    assert_eq!(messages[0].tokens.input, 14925);
    assert_eq!(messages[0].tokens.output, 60);
    assert_eq!(messages[0].tokens.cache_read, 0);
    assert_eq!(messages[0].tokens.reasoning, 863);
    assert_eq!(messages[0].tokens.total(), 15848);
}

#[test]
fn test_parse_gemini_stream_jsonl_replaces_duplicate_message_id() {
    let content = r#"{"type":"gemini","id":"msg-1","model":"gemini-3.1-pro-preview","tokens":{"input":10,"output":1,"cached":0,"thoughts":0,"tool":0,"total":11}}
{"type":"gemini","id":"msg-1","model":"gemini-3.1-pro-preview","tokens":{"input":20,"output":2,"cached":5,"thoughts":3,"tool":0,"total":25}}"#;
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("session-abc.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-3.1-pro-preview");
    assert_eq!(messages[0].tokens.input, 15);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 3);
    assert_eq!(messages[0].tokens.total(), 25);
}

#[test]
fn test_parse_gemini_stream_jsonl_empty_file_returns_no_messages() {
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("empty.jsonl");
    std::fs::write(&file_path, b"").unwrap();

    let messages = parse_gemini_file(&file_path);

    assert!(messages.is_empty());
}

#[test]
fn test_parse_gemini_stream_jsonl_skips_corrupt_lines() {
    let content =
        b"{\"type\":\"init\",\"model\":\"gemini-2.5-pro\",\"session_id\":\"session-1\"}\n\
not-json\n\
{\"type\":\"result\",\"stats\":{\"input_tokens\":10,\"output_tokens\":20}}\n";
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("corrupt.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let result = parse_gemini_file_with_cache_status(&file_path);

    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].session_id, "session-1");
    assert_eq!(result.messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(result.messages[0].tokens.input, 10);
    assert_eq!(result.messages[0].tokens.output, 20);
    assert!(!result.cacheable);
}

#[test]
fn test_parse_gemini_stream_jsonl_skips_truncated_final_line() {
    let content =
        b"{\"type\":\"init\",\"model\":\"gemini-2.5-pro\",\"session_id\":\"session-1\"}\n\
{\"type\":\"result\",\"stats\":{\"input_tokens\":10,\"output_tokens\":20}}\n\
{\"type\":\"result\",\"stats\":{\"input_tokens\":99";
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("truncated.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let result = parse_gemini_file_with_cache_status(&file_path);

    assert_eq!(result.messages.len(), 1);
    assert_eq!(result.messages[0].model_id, "gemini-2.5-pro");
    assert_eq!(result.messages[0].tokens.input, 10);
    assert_eq!(result.messages[0].tokens.output, 20);
    assert!(!result.cacheable);
}

#[test]
fn test_parse_gemini_stream_jsonl_mixed_valid_invalid_lines_preserves_duplicate_replacement() {
    let content = b"{\"type\":\"init\",\"model\":\"gemini-3.1-pro-preview\",\"session_id\":\"session-1\"}\n\
{\"type\":\"gemini\",\"id\":\"msg-1\",\"model\":\"gemini-3.1-pro-preview\",\"tokens\":{\"input\":10,\"output\":1,\"cached\":0,\"thoughts\":0,\"tool\":0,\"total\":11}}\n\
\xff\n\
{\"type\":\"gemini\",\"id\":\"msg-1\",\"model\":\"gemini-3.1-pro-preview\",\"tokens\":{\"input\":20,\"output\":2,\"cached\":5,\"thoughts\":3,\"tool\":0,\"total\":25}}\n\
{\"type\":\"result\",\"stats\":{\"input_tokens\":7,\"output_tokens\":8}}\n";
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("mixed.jsonl");
    std::fs::write(&file_path, content).unwrap();

    let messages = parse_gemini_file(&file_path);

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].session_id, "session-1");
    assert_eq!(messages[0].model_id, "gemini-3.1-pro-preview");
    assert_eq!(messages[0].tokens.input, 15);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 3);
    assert_eq!(messages[1].tokens.input, 7);
    assert_eq!(messages[1].tokens.output, 8);
}

#[test]
fn test_parse_gemini_stream_jsonl_unreadable_file_returns_no_messages() {
    let dir = TempDir::new().unwrap();
    let chats_dir = dir.path().join(".gemini/tmp/123/chats");
    std::fs::create_dir_all(&chats_dir).unwrap();
    let file_path = chats_dir.join("missing.jsonl");

    let messages = parse_gemini_file(&file_path);

    assert!(messages.is_empty());
}

#[test]
fn test_parse_gemini_json_direct_tokens() {
    let json = r#"{"type":"gemini","model":"gemini-3.1-pro-preview","tokens":{"input":20,"output":2,"cached":5,"thoughts":3,"tool":4,"total":29}}"#;
    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gemini-3.1-pro-preview");
    assert_eq!(messages[0].tokens.input, 19);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[0].tokens.reasoning, 3);
    assert_eq!(messages[0].tokens.total(), 29);
}

#[test]
fn test_parse_headless_json_clamps_cached_input_overlap() {
    let json = r#"{"response":"Hi","stats":{"models":{"gemini-2.5-pro":{"tokens":{"prompt":5,"candidates":2,"cached":10}}}}}"#;
    let file = tempfile::Builder::new()
        .prefix("session-")
        .suffix(".json")
        .tempfile()
        .unwrap();
    std::fs::write(file.path(), json).unwrap();

    let messages = parse_gemini_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 0);
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].tokens.cache_read, 10);
    assert_eq!(messages[0].tokens.total(), 12);
}
