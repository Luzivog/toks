use super::*;
use std::path::PathBuf;

#[test]
fn extract_session_content_dispatches_to_real_claude_extractor() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("sess.jsonl");
    std::fs::write(
        &path,
        r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Add a CLI flag"}]}}
"#,
    )
    .unwrap();

    let content = extract_session_content("claude", "sess", &[path]);
    assert_eq!(
        content.first_user_message.as_deref(),
        Some("Add a CLI flag")
    );
    assert_eq!(content.client, "claude");
}

#[test]
fn extract_session_content_unknown_client_is_metadata_only() {
    let content = extract_session_content("totally-unknown", "sess", &[PathBuf::from("/nope")]);
    assert!(content.first_user_message.is_none());
    assert_eq!(content.client, "totally-unknown");
}

#[test]
fn extract_session_content_no_candidates_is_metadata_only() {
    let content = extract_session_content("claude", "sess", &[]);
    assert!(content.first_user_message.is_none());
    assert_eq!(content.client, "claude");
}

#[test]
fn extract_session_content_unreadable_file_does_not_panic() {
    // Missing/unparseable candidate must degrade gracefully, never panic.
    let content = extract_session_content(
        "codex",
        "sess",
        &[PathBuf::from("/definitely/missing.jsonl")],
    );
    assert!(content.first_user_message.is_none());
    assert_eq!(content.client, "codex");
}

#[test]
fn extract_codex_content_parses_current_event_msg_format() {
    // Current on-disk shape: event_msg / payload.type == user_message, with
    // the human text in payload.message.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rollout.jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"type":"event_msg","payload":{"type":"environment_context"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"<environment_context>cwd=/tmp</environment_context>"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Refactor the parser"}}"#,
            "\n",
        ),
    )
    .unwrap();

    let content = extract_codex_content(&path, "sess").unwrap();
    // The system-injected user_message must be skipped; the real prompt wins.
    assert_eq!(
        content.first_user_message.as_deref(),
        Some("Refactor the parser")
    );
    assert_eq!(content.client, "codex");
}

#[test]
fn extract_codex_content_skips_only_injected_returns_none() {
    // A transcript with only harness-injected context yields no human turn.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rollout.jsonl");
    std::fs::write(
        &path,
        concat!(
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"<system-reminder>be concise</system-reminder>"}}"#,
            "\n",
        ),
    )
    .unwrap();

    let content = extract_codex_content(&path, "sess").unwrap();
    assert!(content.first_user_message.is_none());
}

#[test]
fn extract_gemini_content_parses_chat_recording_format() {
    // Chat recording: single JSON doc with a messages array; user turns use
    // {"type":"user","content":"..."}.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-2026.json");
    std::fs::write(
        &path,
        r#"{"sessionId":"b8d9ab56","messages":[{"type":"user","content":"Review the patch"},{"type":"gemini","content":"sure"}]}"#,
    )
    .unwrap();

    let content = extract_gemini_content(&path, "b8d9ab56").unwrap();
    assert_eq!(
        content.first_user_message.as_deref(),
        Some("Review the patch")
    );
    assert_eq!(content.client, "gemini");
}

#[test]
fn extract_gemini_content_empty_user_text_yields_none() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-2026.json");
    std::fs::write(
        &path,
        r#"{"sessionId":"x","messages":[{"type":"user","content":"   "}]}"#,
    )
    .unwrap();

    let content = extract_gemini_content(&path, "x").unwrap();
    assert!(content.first_user_message.is_none());
}

#[test]
fn extract_session_content_empty_message_keeps_scanning_to_real_text() {
    // First candidate parses but its only user message is whitespace; the
    // dispatcher must not accept it as success and must fall through to the
    // second candidate that holds real text.
    let dir = tempfile::tempdir().unwrap();
    let empty = dir.path().join("empty.jsonl");
    std::fs::write(
        &empty,
        r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"   "}]}}
"#,
    )
    .unwrap();
    let real = dir.path().join("real.jsonl");
    std::fs::write(
        &real,
        r#"{"type":"user","message":{"role":"user","content":[{"type":"text","text":"Real prompt"}]}}
"#,
    )
    .unwrap();

    let content = extract_session_content("claude", "sess", &[empty, real]);
    assert_eq!(content.first_user_message.as_deref(), Some("Real prompt"));
}
