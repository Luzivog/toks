use super::*;
use crate::IdentityStrength;
use std::io::Cursor;

fn parse(content: &str, fallback_session_id: &str) -> Vec<UnifiedMessage> {
    parse_codex_reader(
        Cursor::new(content.as_bytes()),
        fallback_session_id,
        0,
        0,
        CodexParseState::default(),
    )
    .messages
}

#[test]
fn identity_excludes_model_and_accounting_facts() {
    let first = r#"{"type":"session_meta","payload":{"id":"logical-session"}}
{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.5"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":1},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}"#;
    let corrected = r#"{"type":"session_meta","payload":{"id":"logical-session"}}
{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.6"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":999,"output_tokens":99},"last_token_usage":{"input_tokens":999,"output_tokens":99}}}}"#;
    let first = parse(first, "copy-a").pop().unwrap();
    let corrected = parse(corrected, "copy-b").pop().unwrap();
    assert_eq!(first.durable_identity, corrected.durable_identity);
    assert_ne!(first.dedup_key, corrected.dedup_key);
    assert_eq!(
        first.durable_identity.unwrap().strength,
        IdentityStrength::SessionStable
    );
}

#[test]
fn copied_fork_lineage_event_keeps_the_same_identity() {
    let mut first = CodexIdentityTracker::default();
    let mut copied = first.clone();
    assert_eq!(
        first.next(
            Some("parent-session"),
            "child-session",
            Some("2026-01-01T00:00:01Z")
        ),
        copied.next(
            Some("parent-session"),
            "child-session",
            Some("2026-01-01T00:00:01Z")
        )
    );
}

#[test]
fn same_fact_events_at_one_timestamp_receive_distinct_identities() {
    let content = r#"{"type":"session_meta","payload":{"id":"logical-session"}}
{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.6"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":1},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"output_tokens":2},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}"#;
    let messages = parse(content, "copy-a");
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens, messages[1].tokens);
    assert_ne!(messages[0].durable_identity, messages[1].durable_identity);
}

#[test]
fn occurrence_survives_incremental_parse() {
    let prefix = r#"{"type":"session_meta","payload":{"id":"logical-session"}}
{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.6"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":1},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}
"#;
    let suffix = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"output_tokens":2},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}
"#;
    let full = parse(&format!("{prefix}{suffix}"), "copy-a");
    let first = parse_codex_reader(
        Cursor::new(prefix.as_bytes()),
        "copy-a",
        0,
        0,
        CodexParseState::default(),
    );
    let second = parse_codex_reader(
        Cursor::new(suffix.as_bytes()),
        "copy-a",
        0,
        prefix.len() as u64,
        first.state,
    );
    assert_eq!(
        full[1].durable_identity,
        second.messages[0].durable_identity
    );
}

#[test]
fn repeated_turn_sub_id_does_not_collapse_identities() {
    let content = r#"{"type":"session_meta","payload":{"id":"logical-session"}}
{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.6"}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"id":"turn-sub-id","type":"token_count","info":{"total_token_usage":{"input_tokens":10,"output_tokens":1},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}
{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"id":"turn-sub-id","type":"token_count","info":{"total_token_usage":{"input_tokens":20,"output_tokens":2},"last_token_usage":{"input_tokens":10,"output_tokens":1}}}}"#;
    let messages = parse(content, "copy-a");
    assert_eq!(messages.len(), 2);
    assert_ne!(messages[0].durable_identity, messages[1].durable_identity);
    assert!(messages.iter().all(|message| message
        .durable_identity
        .as_ref()
        .is_some_and(|id| id.strength == IdentityStrength::SessionStable)));
}
