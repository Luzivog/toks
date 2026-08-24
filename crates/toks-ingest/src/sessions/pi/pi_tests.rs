use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn create_test_file(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    file
}

#[test]
fn test_parse_pi_jsonl_valid_assistant_message() {
    // given
    let content = r#"{"type":"session","id":"pi_ses_001","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","id":"msg_001","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"assistant","model":"claude-3-5-sonnet","provider":"anthropic","usage":{"input":100,"output":50,"cacheRead":10,"cacheWrite":5,"totalTokens":165}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "pi");
    assert_eq!(messages[0].session_id, "pi_ses_001");
    assert_eq!(messages[0].model_id, "claude-3-5-sonnet");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].tokens.cache_read, 10);
    assert_eq!(messages[0].tokens.cache_write, 5);
    assert_eq!(messages[0].workspace_key, Some("/tmp".to_string()));
    assert_eq!(messages[0].workspace_label, Some("tmp".to_string()));
}

#[test]
fn test_parse_pi_infers_provider_from_model_when_absent() {
    // given: no "provider" key at all — a missing provider must be
    // inferred from the model name (gpt-5 -> openai), not hardcoded
    // to "pi".
    let content = r#"{"type":"session","id":"pi_ses_005","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","id":"msg_001","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"assistant","model":"gpt-5","usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-5");
    assert_eq!(messages[0].provider_id, "openai");
}

#[test]
fn test_parse_pi_infers_provider_from_model_when_blank() {
    // given: "provider" present but blank — same inference path as
    // fully absent.
    let content = r#"{"type":"session","id":"pi_ses_006","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","id":"msg_001","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"assistant","model":"gpt-5","provider":"","usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "openai");
}

#[test]
fn test_parse_pi_falls_back_to_pi_when_provider_unrecoverable() {
    // given: no provider and a model name inference can't identify —
    // falls back to "pi" rather than dropping the message.
    let content = r#"{"type":"session","id":"pi_ses_007","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","id":"msg_001","parentId":null,"timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"assistant","model":"totally-unrecognized-model-xyz","usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "pi");
}

#[test]
fn test_parse_pi_subagent_session_name_as_agent() {
    let content = r#"{"type":"session","id":"pi_subagent_001","timestamp":"2026-07-10T00:00:00.000Z","cwd":"/tmp"}
{"type":"session_info","id":"info_001","parentId":null,"timestamp":"2026-07-10T00:00:00.100Z","name":"subagent-go-reviewer-e2e7405c-cb84-4f0a-a6da-9d987494d130-1"}
{"type":"message","id":"msg_001","parentId":"info_001","timestamp":"2026-07-10T00:00:01.000Z","message":{"role":"assistant","model":"gpt-5","provider":"openai","usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}"#;
    let file = create_test_file(content);

    let messages = parse_pi_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent.as_deref(), Some("go-reviewer"));
    assert_eq!(
        pi_subagent_name("subagent-context-builder-208242ce-1").as_deref(),
        Some("context-builder")
    );
    assert_eq!(pi_subagent_name("Refactor auth module"), None);
}

#[test]
fn test_parse_pi_skips_non_assistant_messages() {
    // given
    let content = r#"{"type":"session","id":"pi_ses_002","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"user","model":"claude-3-5-sonnet","provider":"anthropic","usage":{"input":100,"output":50,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert!(messages.is_empty());
}

#[test]
fn test_parse_pi_skips_missing_usage() {
    // given
    let content = r#"{"type":"session","id":"pi_ses_003","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
{"type":"message","timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"assistant","model":"claude-3-5-sonnet","provider":"anthropic"}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert!(messages.is_empty());
}

#[test]
fn test_parse_pi_skips_malformed_json_lines() {
    // given
    let content = r#"{"type":"session","id":"pi_ses_004","timestamp":"2026-01-01T00:00:00.000Z","cwd":"/tmp"}
not valid json
{"type":"message","timestamp":"2026-01-01T00:00:01.000Z","message":{"role":"assistant","model":"gpt-4o-mini","provider":"openai","usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0,"totalTokens":15}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "gpt-4o-mini");
    assert_eq!(messages[0].provider_id, "openai");
}

#[test]
fn test_parse_pi_skips_leading_title_record() {
    // given: current OMP builds write a `title` metadata record before
    // `session` (tokscope#802) — the parser must skip it, not discard
    // the whole file.
    let content = r#"{"type":"title","v":1,"title":"Comment on GitHub issue","source":"auto","updatedAt":"2026-07-02T18:08:49.723Z"}
{"type":"session","id":"pi_ses_005","timestamp":"2026-07-02T18:07:14.690Z","cwd":"/tmp"}
{"type":"message","timestamp":"2026-07-02T18:08:53.229Z","message":{"role":"assistant","model":"claude-sonnet-5","provider":"anthropic","usage":{"input":2,"output":180,"cacheRead":0,"cacheWrite":70844,"totalTokens":71026}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "pi_ses_005");
    assert_eq!(messages[0].model_id, "claude-sonnet-5");
    assert_eq!(messages[0].provider_id, "anthropic");
    assert_eq!(messages[0].tokens.input, 2);
    assert_eq!(messages[0].tokens.output, 180);
    assert_eq!(messages[0].tokens.cache_write, 70844);
}

#[test]
fn test_parse_pi_skips_multiple_leading_title_records() {
    // given: defensive against more than one pre-session metadata line
    // in a row (e.g. a title record rewritten by a later auto-rename).
    let content = r#"{"type":"title","v":1,"title":"first"}
{"type":"title","v":1,"title":"renamed"}
{"type":"session","id":"pi_ses_006","timestamp":"2026-07-02T18:07:14.690Z","cwd":"/tmp"}
{"type":"message","timestamp":"2026-07-02T18:08:53.229Z","message":{"role":"assistant","model":"gpt-4o-mini","provider":"openai","usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0,"totalTokens":15}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "pi_ses_006");
}

#[test]
fn test_parse_pi_rejects_unknown_leading_record_type() {
    // given: an unrecognized type before `session` is still treated as
    // a malformed file rather than silently scanned through.
    let content = r#"{"type":"totally_unknown_thing","foo":"bar"}
{"type":"session","id":"pi_ses_007","timestamp":"2026-07-02T18:07:14.690Z","cwd":"/tmp"}
{"type":"message","timestamp":"2026-07-02T18:08:53.229Z","message":{"role":"assistant","model":"gpt-4o-mini","provider":"openai","usage":{"input":10,"output":5,"cacheRead":0,"cacheWrite":0,"totalTokens":15}}}"#;
    let file = create_test_file(content);

    // when
    let messages = parse_pi_file(file.path());

    // then
    assert!(messages.is_empty());
}
