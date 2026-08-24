use super::*;

fn create_transcript_file(content: &str, filename: &str) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join(".claude")
        .join("transcripts")
        .join(filename);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();
    (temp_dir, path)
}

#[test]
fn test_workspace_metadata_from_claude_project_path() {
    let content = r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","message":{"model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let (_dir, path) = create_project_file(content, "myproject", "session.jsonl");

    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key, Some("myproject".to_string()));
    assert_eq!(messages[0].workspace_label, Some("myproject".to_string()));
}

#[test]
fn test_wrapper_transcript_with_usage_is_parsed() {
    let content = r#"{"type":"user","timestamp":"2026-04-01T10:00:00.000Z","message":{"content":"Wrapped prompt"}}
{"type":"assistant","timestamp":"2026-04-01T10:00:01.000Z","requestId":"req_wrapper","message":{"id":"msg_wrapper","model":"claude-sonnet-4","usage":{"input_tokens":123,"output_tokens":45,"cache_read_input_tokens":67,"cache_creation_input_tokens":8}}}"#;
    let (_dir, path) = create_transcript_file(content, "ses_123456789012345678901234567.jsonl");

    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "ses_123456789012345678901234567");
    assert_eq!(messages[0].model_id, "claude-sonnet-4");
    assert_eq!(messages[0].tokens.input, 123);
    assert_eq!(messages[0].tokens.output, 45);
    assert_eq!(messages[0].tokens.cache_read, 67);
    assert_eq!(messages[0].tokens.cache_write, 8);
    assert_eq!(messages[0].workspace_key, None);
    assert_eq!(messages[0].workspace_label, None);
}

#[test]
fn test_wrapper_transcript_without_usage_is_skipped() {
    let content = r#"{"type":"user","timestamp":"2026-04-01T10:00:00.000Z","message":{"content":"Wrapped prompt"}}
{"type":"tool_use","timestamp":"2026-04-01T10:00:01.000Z","message":{"content":"Run tool"}}
{"type":"tool_result","timestamp":"2026-04-01T10:00:02.000Z","message":{"content":"Tool result"}}"#;
    let (_dir, path) = create_transcript_file(content, "ses_765432109876543210987654321.jsonl");

    let messages = parse_claude_file(&path);

    assert!(
        messages.is_empty(),
        "wrapper transcripts without usage metadata must not be estimated"
    );
}

#[test]
fn test_bare_transcript_with_tool_outputs_is_not_estimated() {
    let content = r#"{"type":"tool_use","timestamp":"2026-04-01T10:00:00.000Z","tool_name":"read","tool_input":{"filePath":"/src/main.rs"}}
{"type":"tool_result","timestamp":"2026-04-01T10:00:01.000Z","tool_name":"read","tool_input":{"filePath":"/src/main.rs"},"tool_output":{"output":"fn main() {\n    println!(\"Hello, world!\");\n}\n"}}
{"type":"tool_use","timestamp":"2026-04-01T10:00:02.000Z","tool_name":"bash","tool_input":{"command":"cargo build"}}
{"type":"tool_result","timestamp":"2026-04-01T10:00:03.000Z","tool_name":"bash","tool_input":{"command":"cargo build"},"tool_output":{"output":"   Compiling myproject v0.1.0\n    Finished dev [unoptimized + debuginfo] target(s) in 2.34s\n"}}"#;
    let (_dir, path) = create_transcript_file(content, "ses_aabbccdd11223344556677889.jsonl");

    let messages = parse_claude_file(&path);

    assert!(
        messages.is_empty(),
        "bare transcripts with only tool outputs must not produce estimated token messages"
    );
}

#[test]
fn test_project_transcript_with_tool_outputs_is_not_char_estimated() {
    // Same rule as bare transcripts (tokscope#1011): project sessions'
    // assistant usage already includes tool_result text.
    let content = r#"{"type":"tool_result","timestamp":"2026-04-01T10:00:01.000Z","message":{"role":"user","content":[{"type":"tool_result","tool_use_id":"toolu_001","content":[{"type":"text","text":"fn main() { println!(\"hello\"); }"}]}]}}"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("myproject")
        .join("ses_project123.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();

    let messages = parse_claude_file(&path);

    assert!(
        messages.is_empty(),
        "project transcripts must not char-estimate tool_result rows without explicit tokens"
    );
}

#[test]
fn test_bare_transcript_with_explicit_tool_result_tokens_is_counted() {
    // Bare transcripts must not char-estimate tokens, but explicit tool-result
    // token counts (e.g. reported by the originating client) should still be honored.
    let content = r#"{"type":"tool_result","timestamp":"2026-04-01T10:00:01.000Z","tool_name":"read","input_tokens":42,"tool_output":{"output":"fn main() {\n    println!(\"Hello, world!\");\n}\n"}}"#;
    let (_dir, path) = create_transcript_file(content, "ses_explicit112233445566778899.jsonl");

    let messages = parse_claude_file(&path);

    assert_eq!(
        messages.len(),
        1,
        "bare transcripts must still count explicit tool-result token usage"
    );
    assert_eq!(messages[0].tokens.input, 42);
}

#[test]
fn test_transcripts_dir_under_project_keeps_workspace_attribution() {
    // A `transcripts/` directory nested under a resolvable `projects/<key>/`
    // path must still resolve workspace attribution. Char estimation is off
    // everywhere now (#1011), so pin the workspace via an assistant usage row.
    let content = r#"{"type":"assistant","timestamp":"2026-04-01T10:00:01.000Z","message":{"model":"claude-sonnet-4-6","usage":{"input_tokens":10,"output_tokens":2}}}"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join("projects")
        .join("myproject")
        .join("transcripts")
        .join("ses_scoped112233445566778899.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, content).unwrap();

    let messages = parse_claude_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key, Some("myproject".to_string()));
    assert_eq!(messages[0].tokens.input, 10);
}
