use super::*;
use std::io::Write;
use std::path::Path;
use tempfile::{NamedTempFile, TempDir};

fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

fn create_test_file_with_name(content: &str, filename: &str) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join(format!("test_project/chats/{}.jsonl", filename));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    (temp_dir, path)
}

fn create_project_test_file(
    content: &str,
    project: &str,
    filename: &str,
) -> (TempDir, std::path::PathBuf) {
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join(format!("projects/{project}/chats/{filename}.jsonl"));
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    (temp_dir, path)
}

#[test]
fn test_parse_qwen_valid_assistant_message() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "d96bf338", "usageMetadata": {"promptTokenCount": 12414, "candidatesTokenCount": 76, "thoughtsTokenCount": 39, "cachedContentTokenCount": 0}}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "qwen");
    assert_eq!(messages[0].model_id, "qwen3.5-plus");
    assert_eq!(messages[0].provider_id, "qwen");
    // Session ID comes from filename, not JSON content (temp file has random name)
    assert!(!messages[0].session_id.is_empty());
    assert_eq!(messages[0].tokens.input, 12414);
    assert_eq!(messages[0].tokens.output, 76);
    assert_eq!(messages[0].tokens.reasoning, 39);
    assert_eq!(messages[0].tokens.cache_read, 0);
    assert_eq!(messages[0].tokens.cache_write, 0);
    assert_eq!(messages[0].workspace_key, None);
    assert_eq!(messages[0].workspace_label, None);
}

#[test]
fn test_parse_qwen_multi_turn() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}
{"type": "assistant", "model": "qwen3-coder-plus", "timestamp": "2026-02-23T14:25:00.000Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 300, "candidatesTokenCount": 400, "thoughtsTokenCount": 20, "cachedContentTokenCount": 10}}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "qwen3.5-plus");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 200);
    assert_eq!(messages[0].tokens.reasoning, 10);
    assert_eq!(messages[0].tokens.cache_read, 5);
    assert_eq!(messages[1].model_id, "qwen3-coder-plus");
    assert_eq!(messages[1].tokens.input, 300);
    assert_eq!(messages[1].tokens.output, 400);
    assert_eq!(messages[1].tokens.reasoning, 20);
    assert_eq!(messages[1].tokens.cache_read, 10);
}

#[test]
fn test_workspace_metadata_from_qwen_project_path() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "d96bf338", "usageMetadata": {"promptTokenCount": 12414, "candidatesTokenCount": 76, "thoughtsTokenCount": 39, "cachedContentTokenCount": 0}}"#;
    let (_dir, path) = create_project_test_file(content, "test_project", "abc123");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key, Some("test_project".to_string()));
    assert_eq!(
        messages[0].workspace_label,
        Some("test_project".to_string())
    );
}

#[test]
fn test_workspace_metadata_ignores_unanchored_projects_segments() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "d96bf338", "usageMetadata": {"promptTokenCount": 12414, "candidatesTokenCount": 76, "thoughtsTokenCount": 39, "cachedContentTokenCount": 0}}"#;
    let temp_dir = tempfile::tempdir().unwrap();
    let path = temp_dir
        .path()
        .join("projects/noise/not-chats/demo/.qwen/projects/real_project/chats/abc123.jsonl");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].workspace_key.as_deref(), Some("real_project"));
    assert_eq!(messages[0].workspace_label.as_deref(), Some("real_project"));
}

#[test]
fn test_parse_qwen_skip_non_assistant() {
    let content = r#"{"type": "user", "timestamp": "2026-02-23T14:24:50.000Z", "content": "Hello"}
{"type": "system", "timestamp": "2026-02-23T14:24:51.000Z", "subtype": "ui_telemetry"}
{"type": "tool_result", "timestamp": "2026-02-23T14:24:52.000Z", "result": "success"}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_parse_qwen_empty_file() {
    let file = create_test_file("");

    let messages = parse_qwen_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_parse_qwen_malformed_lines() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}
not valid json at all
{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:25:00.000Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 300, "candidatesTokenCount": 400, "thoughtsTokenCount": 20, "cachedContentTokenCount": 10}}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[1].tokens.input, 300);
}

#[test]
fn test_parse_qwen_skips_zero_token_entries() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 0, "candidatesTokenCount": 0, "thoughtsTokenCount": 0, "cachedContentTokenCount": 0}}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert!(messages.is_empty());
}

#[test]
fn test_parse_qwen_with_cache_and_reasoning() {
    let content = r#"{"type": "assistant", "model": "qwen3-max-2026-01-23", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 1508, "candidatesTokenCount": 205, "thoughtsTokenCount": 50, "cachedContentTokenCount": 4864}}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 1508);
    assert_eq!(messages[0].tokens.output, 205);
    assert_eq!(messages[0].tokens.reasoning, 50);
    assert_eq!(messages[0].tokens.cache_read, 4864);
    assert_eq!(messages[0].tokens.cache_write, 0);
}

#[test]
fn test_parse_qwen_unknown_model_fallback() {
    let content = r#"{"type": "assistant", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "session1", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}"#;
    let file = create_test_file(content);

    let messages = parse_qwen_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "unknown");
    assert_eq!(messages[0].tokens.input, 100);
}

#[test]
fn test_session_id_from_json_when_present() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "abc123def456", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}"#;
    let (_dir, path) = create_test_file_with_name(content, "json_present");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 1);
    // Should use the sessionId from JSON, not the filename
    assert_eq!(messages[0].session_id, "abc123def456");
}

#[test]
fn test_session_id_fallback_when_empty_string() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}"#;
    let (_dir, path) = create_test_file_with_name(content, "json_empty");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 1);
    // Should fallback to path-derived ID (not empty string)
    assert!(!messages[0].session_id.is_empty());
    assert_ne!(messages[0].session_id, "");
    // Verify it's not the JSON empty value
    assert_ne!(messages[0].session_id, "");
}

#[test]
fn test_session_id_fallback_when_missing() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}"#;
    let (_dir, path) = create_test_file_with_name(content, "json_missing");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 1);
    // Should fallback to path-derived ID
    assert!(!messages[0].session_id.is_empty());
}

#[test]
fn test_session_id_fallback_when_null() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": null, "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}"#;
    let (_dir, path) = create_test_file_with_name(content, "json_null");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 1);
    // Should fallback to path-derived ID
    assert!(!messages[0].session_id.is_empty());
    assert_ne!(messages[0].session_id, "null");
}

#[test]
fn test_cross_project_session_id_uniqueness() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}"#;

    // Create two files with same name in different projects
    let (_dir1, path1) = create_test_file_with_name(content, "session");

    // Manually create a second file in a different project
    let temp_dir = tempfile::tempdir().unwrap();
    let path2 = temp_dir.path().join("other_project/chats/session.jsonl");
    std::fs::create_dir_all(path2.parent().unwrap()).unwrap();
    let mut file2 = std::fs::File::create(&path2).unwrap();
    file2.write_all(content.as_bytes()).unwrap();

    let messages1 = parse_qwen_file(&path1);
    let messages2 = parse_qwen_file(&path2);

    assert_eq!(messages1.len(), 1);
    assert_eq!(messages2.len(), 1);

    // Session IDs should be different despite same filename
    assert_ne!(messages1[0].session_id, messages2[0].session_id);
}

#[test]
fn test_extract_session_id_with_fallback_uses_json_value() {
    let path = Path::new("/home/user/.qwen/projects/myapp/chats/abc123.jsonl");
    let json_session_id = Some("json_session_456");

    let result = extract_session_id_with_fallback(path, json_session_id);

    assert_eq!(result, "json_session_456");
}

#[test]
fn test_extract_session_id_with_fallback_empty_uses_path() {
    let path = Path::new("/home/user/.qwen/projects/myapp/chats/abc123.jsonl");
    let json_session_id = Some("");

    let result = extract_session_id_with_fallback(path, json_session_id);

    // Should use path-derived ID containing project and filename
    assert!(result.contains("myapp") || result.contains("abc123"));
}

#[test]
fn test_extract_session_id_with_fallback_none_uses_path() {
    let path = Path::new("/home/user/.qwen/projects/myapp/chats/abc123.jsonl");
    let json_session_id: Option<&str> = None;

    let result = extract_session_id_with_fallback(path, json_session_id);

    // Should use path-derived ID containing project and filename
    assert!(result.contains("myapp") || result.contains("abc123"));
}

#[test]
fn test_path_derived_session_id_includes_project() {
    let path = Path::new("/home/user/.qwen/projects/some-project/chats/chat-session.jsonl");
    let result = extract_session_id_with_fallback(path, None);

    // Should include both project name and filename stem
    assert!(
        result.contains("some-project"),
        "Session ID should contain project name"
    );
    assert!(
        result.contains("chat-session"),
        "Session ID should contain filename"
    );
}

#[test]
fn test_multi_turn_same_session_id() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "shared_session", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}
{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:25:00.000Z", "sessionId": "shared_session", "usageMetadata": {"promptTokenCount": 300, "candidatesTokenCount": 400, "thoughtsTokenCount": 20, "cachedContentTokenCount": 10}}"#;
    let (_dir, path) = create_test_file_with_name(content, "multi");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 2);
    // Both messages should have the same session ID from JSON
    assert_eq!(messages[0].session_id, "shared_session");
    assert_eq!(messages[1].session_id, "shared_session");
}

#[test]
fn test_mixed_session_id_in_file() {
    let content = r#"{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:24:56.857Z", "sessionId": "valid_id", "usageMetadata": {"promptTokenCount": 100, "candidatesTokenCount": 200, "thoughtsTokenCount": 10, "cachedContentTokenCount": 5}}
{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:25:00.000Z", "usageMetadata": {"promptTokenCount": 300, "candidatesTokenCount": 400, "thoughtsTokenCount": 20, "cachedContentTokenCount": 10}}
{"type": "assistant", "model": "qwen3.5-plus", "timestamp": "2026-02-23T14:26:00.000Z", "sessionId": "", "usageMetadata": {"promptTokenCount": 500, "candidatesTokenCount": 600, "thoughtsTokenCount": 30, "cachedContentTokenCount": 15}}"#;
    let (_dir, path) = create_test_file_with_name(content, "mixed");

    let messages = parse_qwen_file(&path);

    assert_eq!(messages.len(), 3);
    // First message uses JSON sessionId
    assert_eq!(messages[0].session_id, "valid_id");
    // Second message (no sessionId) uses fallback
    assert!(
        messages[1].session_id.contains("mixed") || messages[1].session_id.contains("test_project")
    );
    // Third message (empty sessionId) uses fallback
    assert!(
        messages[2].session_id.contains("mixed") || messages[2].session_id.contains("test_project")
    );
}
