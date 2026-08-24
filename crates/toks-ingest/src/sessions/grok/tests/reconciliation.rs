use super::*;

#[test]
fn adds_signals_reconciliation_when_compaction_exceeds_updates() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-build"}},"_meta":{"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":171056,"agentTimestampMs":1700000001000}}}"#,
        None,
        Some(
            r#"{"primaryModelId":"grok-build","totalTokensBeforeCompaction":3224659,"contextTokensUsed":172309}"#,
        ),
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 171056);
    assert_eq!(messages[1].tokens.input, 3225912);
    assert_eq!(messages[1].model_id, "grok-build");
    assert_eq!(
        messages[1].dedup_key.as_deref(),
        Some("grok:session-1:signals")
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.input)
            .sum::<i64>(),
        3396968
    );
}

#[test]
fn signals_reconciliation_anchors_timestamp_to_last_update_not_file_mtime() {
    // The signals.json is written "now" (mtime far in the future relative to
    // the update timestamps). The reconciliation delta must be dated by the
    // last recorded update (1700000001000), NOT the signals.json mtime, so a
    // live session's extra does not migrate to a new day on every rescan.
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk","_meta":{"modelId":"grok-build"}},"_meta":{"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":171056,"agentTimestampMs":1700000001000}}}"#,
        None,
        Some(
            r#"{"primaryModelId":"grok-build","totalTokensBeforeCompaction":3224659,"contextTokensUsed":172309}"#,
        ),
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[1].dedup_key.as_deref(),
        Some("grok:session-1:signals")
    );
    assert_eq!(messages[1].timestamp, 1700000001000);
}

#[test]
fn skips_signals_reconciliation_when_updates_already_cover_signals() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"user_message_chunk"},"_meta":{"agentTimestampMs":1700000000000}}}
{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"agent_message_chunk"},"_meta":{"totalTokens":500,"agentTimestampMs":1700000001000}}}"#,
        None,
        Some(r#"{"primaryModelId":"grok-build","contextTokensUsed":400}"#),
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 500);
}

#[test]
fn uses_signals_model_when_updates_model_is_missing() {
    let (_temp, path) = write_fixture(
        r#"{"method":"session/update","params":{"sessionId":"session-1","update":{"sessionUpdate":"available_commands_update"},"_meta":{"totalTokens":50,"agentTimestampMs":1700000000000}}}"#,
        None,
        Some(r#"{"primaryModelId":"grok-composer-2.5-fast","contextTokensUsed":250}"#),
    );

    let messages = parse_grok_updates_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 50);
    assert_eq!(messages[1].tokens.input, 200);
    assert_eq!(messages[1].model_id, "grok-composer-2.5-fast");
}
