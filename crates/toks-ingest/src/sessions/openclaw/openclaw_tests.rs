use super::*;
use std::fs::File;
use std::io::Write;
use tempfile::TempDir;

fn create_test_session(dir: &TempDir, filename: &str, content: &str) -> String {
    let path = dir.path().join(filename);
    let mut file = File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    path.to_string_lossy().to_string()
}

#[test]
fn test_parse_openclaw_session_with_model_change() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","id":"abc","provider":"openai-codex","modelId":"gpt-5.2"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":100,"output":50,"cacheRead":200,"totalTokens":350,"cost":{"total":0.05}},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5.2");
    assert_eq!(messages[0].provider_id, "openai-codex");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 200);
    assert_eq!(messages[0].cost, 0.05);
}

#[test]
fn test_parse_openclaw_session_user_messages_ignored() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-3.5-sonnet"}
{"type":"message","id":"msg1","message":{"role":"user","content":[{"type":"text","text":"hello"}]}}
{"type":"message","id":"msg2","message":{"role":"assistant","content":[],"usage":{"input":50,"output":25},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 50);
}

#[test]
fn test_parse_openclaw_session_no_model_change() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":100,"output":50},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 0);
}

#[test]
fn test_parse_openclaw_transcript_derives_session_id_from_filename() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","provider":"openai-codex","modelId":"gpt-5.2"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "my-session-123.jsonl", content);
    let messages = parse_openclaw_transcript(Path::new(&session_path));

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "my-session-123");
    assert_eq!(messages[0].model_id, "gpt-5.2");
    assert_eq!(messages[0].provider_id, "openai-codex");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn test_parse_openclaw_transcript_derives_session_id_from_archived_filename() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","provider":"openai-codex","modelId":"gpt-5.2"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0},"timestamp":1700000000000}}"#;

    let session_path =
        create_test_session(&dir, "my-session-123.jsonl.deleted.1700000000000", content);
    let messages = parse_openclaw_transcript(Path::new(&session_path));

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "my-session-123");
    assert_eq!(messages[0].model_id, "gpt-5.2");
    assert_eq!(messages[0].provider_id, "openai-codex");
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn test_parse_openclaw_transcript_derives_session_id_from_reset_filename() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-opus-4-6"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":10,"output":5,"cacheRead":1,"cacheWrite":2},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(
        &dir,
        "my-session-123.jsonl.reset.2026-03-20T06-34-44.520Z",
        content,
    );
    let messages = parse_openclaw_transcript(Path::new(&session_path));

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "my-session-123");
    assert_eq!(messages[0].model_id, "claude-opus-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
}

#[test]
fn test_parse_openclaw_session_model_snapshot_updates_current_model() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"custom","customType":"model-snapshot","data":{"provider":"anthropic","modelId":"claude-opus-4-6"}}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":100,"output":50,"cacheRead":25,"cacheWrite":10},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-opus-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 25);
    assert_eq!(messages[0].tokens.cache_write, 10);
}

#[test]
fn test_parse_openclaw_session_embedded_model_provider_without_model_change() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"message","id":"msg1","message":{"role":"assistant","provider":"anthropic","model":"claude-sonnet-4-6","content":[],"usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":5},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.cache_write, 5);
}

#[test]
fn test_parse_openclaw_session_preserves_unknown_provider_fallback() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","modelId":"claude-sonnet-4-6"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":10,"output":5},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-6");
    assert_eq!(messages[0].provider_id, "unknown");
}

#[test]
fn test_parse_openclaw_session_empty_embedded_values_fall_back_to_current_model_state() {
    let dir = TempDir::new().unwrap();
    let content = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-opus-4-6"}
{"type":"message","id":"msg1","message":{"role":"assistant","provider":"","model":"","content":[],"usage":{"input":10,"output":5},"timestamp":1700000000000}}"#;

    let session_path = create_test_session(&dir, "session.jsonl", content);
    let messages = parse_openclaw_session(Path::new(&session_path), "test-session");

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-opus-4-6");
    assert_eq!(messages[0].provider_id, "anthropic");
}

fn create_test_index(dir: &TempDir, content: &str) -> PathBuf {
    let index_path = dir.path().join("sessions.json");
    let mut file = File::create(&index_path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    index_path
}

#[test]
fn test_parse_openclaw_index_absolute_session_file() {
    let dir = TempDir::new().unwrap();

    let session_content = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-3.5-sonnet"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0},"timestamp":1700000000000}}"#;
    let session_path = create_test_session(&dir, "session-abc.jsonl", session_content);

    let index_content = format!(
        r#"{{
        "agent:main:main": {{
            "sessionId": "abc-123",
            "sessionFile": "{}"
        }}
    }}"#,
        session_path.replace('\\', "\\\\")
    );
    let index_path = create_test_index(&dir, &index_content);

    let messages = parse_openclaw_index(&index_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-3.5-sonnet");
    assert_eq!(messages[0].session_id, "abc-123");
}

#[test]
fn test_parse_openclaw_index_relative_session_file() {
    let dir = TempDir::new().unwrap();

    let session_content = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-3.5-sonnet"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0},"timestamp":1700000000000}}"#;
    create_test_session(&dir, "session-relative.jsonl", session_content);

    let index_content = r#"{
        "agent:main:main": {
            "sessionId": "relative-123",
            "sessionFile": "session-relative.jsonl"
        }
    }"#;
    let index_path = create_test_index(&dir, index_content);

    let messages = parse_openclaw_index(&index_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-3.5-sonnet");
    assert_eq!(messages[0].session_id, "relative-123");
}

#[test]
fn test_parse_openclaw_index_missing_session_file_fallback() {
    let dir = TempDir::new().unwrap();

    let session_content = r#"{"type":"model_change","provider":"anthropic","modelId":"claude-3.5-sonnet"}
{"type":"message","id":"msg1","message":{"role":"assistant","content":[],"usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0},"timestamp":1700000000000}}"#;
    create_test_session(&dir, "fallback-123.jsonl", session_content);

    let index_content = r#"{
        "agent:main:main": {
            "sessionId": "fallback-123"
        }
    }"#;
    let index_path = create_test_index(&dir, index_content);

    let messages = parse_openclaw_index(&index_path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-3.5-sonnet");
    assert_eq!(messages[0].session_id, "fallback-123");
}
