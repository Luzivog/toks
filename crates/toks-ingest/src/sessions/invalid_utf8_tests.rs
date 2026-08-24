use super::claudecode::parse_claude_file;
use super::codex::{parse_codex_file_incremental, CodexParseState};
use super::droid::parse_droid_file;
use super::kiro::parse_kiro_file;
use crate::clients::ClientId;
use crate::message_cache::parser_version;

fn bytes_around_invalid(prefix: &str, suffix: &str) -> Vec<u8> {
    let mut bytes = prefix.as_bytes().to_vec();
    bytes.push(0xff);
    bytes.extend_from_slice(suffix.as_bytes());
    bytes
}

#[test]
fn claudecode_recovers_parent_agent_mapping_from_lossy_json_line() {
    let temp_dir = tempfile::tempdir().unwrap();
    let project_dir = temp_dir
        .path()
        .join(".claude")
        .join("projects")
        .join("project");
    std::fs::create_dir_all(&project_dir).unwrap();
    let parent_id = "parent-session";
    let parent_path = project_dir.join(format!("{parent_id}.jsonl"));
    let parent = bytes_around_invalid(
        concat!(
            r#"{"type":"summary"}"#,
            "\n",
            r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"toolu_1","name":"Agent","input":{"subagent_type":"document-specialist","prompt":"research "#,
        ),
        concat!(
            r#" docs"}}]}}"#,
            "\n",
            r#"{"type":"user","message":{"content":[{"tool_use_id":"toolu_1","type":"tool_result","content":[{"type":"text","text":"agentId: agent1 done"}]}]}}"#,
            "\n"
        ),
    );
    std::fs::write(parent_path, parent).unwrap();

    let subagents_dir = project_dir.join(parent_id).join("subagents");
    std::fs::create_dir_all(&subagents_dir).unwrap();
    let sidechain_path = subagents_dir.join("agent-agent1.jsonl");
    std::fs::write(
        &sidechain_path,
        concat!(
            r#"{"type":"user","isSidechain":true,"sessionId":"parent-session","agentId":"agent1","timestamp":"2026-01-01T00:00:00Z","message":{"content":"research"}}"#,
            "\n",
            r#"{"type":"assistant","isSidechain":true,"sessionId":"parent-session","agentId":"agent1","timestamp":"2026-01-01T00:00:01Z","requestId":"req_1","message":{"id":"msg_1","model":"claude-sonnet-4-5","usage":{"input_tokens":10,"output_tokens":4}}}"#,
            "\n"
        ),
    )
    .unwrap();

    let messages = parse_claude_file(&sidechain_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent.as_deref(), Some("Document Specialist"));
}

#[test]
fn codex_parses_token_count_after_invalid_utf8_and_tracks_raw_offset() {
    let temp_file = tempfile::NamedTempFile::new().unwrap();
    let bytes = bytes_around_invalid(
        concat!(
            r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
            "\n"
        ),
        concat!(
            "\n",
            r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
            "\n"
        ),
    );
    std::fs::write(temp_file.path(), &bytes).unwrap();

    let parsed = parse_codex_file_incremental(temp_file.path(), 0, CodexParseState::default());

    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].tokens.input, 8);
    assert_eq!(parsed.messages[0].tokens.output, 3);
    assert_eq!(parsed.consumed_offset, bytes.len() as u64);
}

#[test]
fn kiro_preserves_prompt_content_with_invalid_utf8_before_assistant_line() {
    let temp_dir = tempfile::tempdir().unwrap();
    let header_path = temp_dir.path().join("session.json");
    std::fs::write(
        &header_path,
        r#"{"session_id":"session","session_state":{"rts_model_state":{"model_info":{"model_id":"claude-sonnet-4-5"}},"conversation_metadata":{"user_turn_metadatas":[{"input_token_count":0,"output_token_count":0,"message_ids":["prompt","assistant"]}]}}}"#,
    )
    .unwrap();
    let messages_path = temp_dir.path().join("session.jsonl");
    let messages = bytes_around_invalid(
        concat!(
            r#"{"version":"v1","kind":"Metadata","data":{}}"#,
            "\n",
            r#"{"version":"v1","kind":"Prompt","data":{"message_id":"prompt","content":[{"kind":"text","data":"hello "#,
        ),
        concat!(
            r#" world"}],"meta":{"timestamp":1770983426.0}}}"#,
            "\n",
            r#"{"version":"v1","kind":"AssistantMessage","data":{"message_id":"assistant","content":[{"kind":"text","data":"response"}]}}"#,
            "\n"
        ),
    );
    std::fs::write(messages_path, messages).unwrap();

    let parsed = parse_kiro_file(&header_path);

    assert_eq!(parsed.len(), 1);
    assert!(parsed[0].tokens.input > 0);
    assert!(parsed[0].tokens.output > 0);
}

#[test]
fn droid_finds_model_after_invalid_utf8_line() {
    let temp_dir = tempfile::tempdir().unwrap();
    let settings_path = temp_dir.path().join("session.settings.json");
    std::fs::write(
        &settings_path,
        r#"{"providerLock":"anthropic","tokenUsage":{"inputTokens":10,"outputTokens":4}}"#,
    )
    .unwrap();
    let jsonl = bytes_around_invalid(
        concat!(r#"{"type":"message","content":"before"}"#, "\n"),
        concat!(
            "\n",
            r#"{"type":"message","content":"Model: Claude Opus 4.5 Thinking [Anthropic]"}"#,
            "\n"
        ),
    );
    std::fs::write(temp_dir.path().join("session.jsonl"), jsonl).unwrap();

    let messages = parse_droid_file(&settings_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude opus 4-5 thinking");
}

#[test]
fn parser_behavior_changes_invalidate_cached_sessions() {
    let expected_versions = [
        (ClientId::Codex, 9),
        (ClientId::Droid, 2),
        (ClientId::OpenClaw, 2),
        (ClientId::Pi, 3),
        (ClientId::Kimi, 5),
        (ClientId::Qwen, 2),
        (ClientId::Copilot, 9),
        (ClientId::Kiro, 3),
        (ClientId::Gjc, 2),
        (ClientId::Jcode, 8),
        (ClientId::CommandCode, 2),
        (ClientId::Junie, 4),
        (ClientId::Zcode, 4),
        (ClientId::OpenCodeReview, 4),
        (ClientId::CodeBuddy, 2),
        (ClientId::WorkBuddy, 2),
        (ClientId::DevinDesktop, 3),
        (ClientId::Senpi, 2),
        (ClientId::Kimchi, 3),
        (ClientId::Reasonix, 2),
        (ClientId::PrimeAgent, 2),
    ];

    for (client, expected) in expected_versions {
        assert_eq!(parser_version(client), expected, "{}", client.as_str());
    }
    assert_eq!(parser_version(ClientId::Claude), 2);
}
