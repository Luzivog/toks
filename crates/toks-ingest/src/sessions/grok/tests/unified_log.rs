use super::*;

#[test]
fn parses_unified_log_token_breakdown_without_double_counting_reasoning() {
    let (_temp, path) = write_unified_fixture(
        r#"{"ts":"2023-11-14T22:13:19Z","pid":17,"sid":"session-1","msg":"model changed","ctx":{"model":"grok-composer-2.5-fast"}}
{"ts":"2023-11-14T22:13:19Z","pid":17,"msg":"model catalog: notifying clients","ctx":{"current_model_id":"grok-4.5"}}
{"ts":"2023-11-14T22:13:20Z","pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"cached_prompt_tokens":60,"completion_tokens":25,"reasoning_tokens":5}}
{"ts":"2023-11-14T22:13:21Z","pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":2,"prompt_tokens":80,"cached_prompt_tokens":0,"completion_tokens":12,"reasoning_tokens":0}}
{"ts":"2023-11-14T22:13:20Z","pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"cached_prompt_tokens":60,"completion_tokens":25,"reasoning_tokens":5}}
{"ts":"2023-11-14T22:13:22Z","pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":3,"prompt_tokens":10,"cached_prompt_tokens":11,"completion_tokens":1,"reasoning_tokens":0}}
{"ts":"2023-11-14T22:13:23Z","pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":4,"prompt_tokens":10,"cached_prompt_tokens":0,"completion_tokens":1,"reasoning_tokens":2}}"#,
    );

    let messages = parse_grok_unified_log_file(&path);

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].client, CLIENT_ID);
    assert_eq!(messages[0].model_id, "grok-composer-2.5-fast");
    assert_eq!(messages[0].session_id, "session-1");
    assert_eq!(messages[0].tokens.input, 40);
    assert_eq!(messages[0].tokens.cache_read, 60);
    assert_eq!(messages[0].tokens.output, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);
    assert_eq!(messages[0].tokens.total(), 125);
    assert_eq!(messages[0].message_count, 1);
    assert!(messages[0].is_turn_start);
    assert_eq!(messages[1].tokens.input, 80);
    assert_eq!(messages[1].tokens.output, 12);
    assert_eq!(messages[1].message_count, 0);
    assert!(!messages[1].is_turn_start);
    assert_eq!(messages[2].tokens.input, 0);
    assert_eq!(messages[2].tokens.cache_read, 10);
    assert_eq!(messages[2].tokens.output, 1);
    assert_eq!(messages[2].tokens.total(), 11);
    assert_eq!(messages[2].message_count, 0);
    assert!(!messages[2].is_turn_start);
    assert_eq!(messages[3].tokens.input, 10);
    assert_eq!(messages[3].tokens.output, 0);
    assert_eq!(messages[3].tokens.reasoning, 1);
    assert_eq!(messages[3].tokens.total(), 11);
    assert_eq!(messages[3].message_count, 0);
    assert!(!messages[3].is_turn_start);
}

#[test]
fn unified_log_keeps_distinct_rows_when_fallback_timestamp_and_tokens_repeat() {
    let (_temp, path) = write_unified_fixture(
        r#"{"pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"completion_tokens":25,"request_id":"first"}}
{"pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"completion_tokens":25,"request_id":"second"}}
{"pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":100,"completion_tokens":25,"request_id":"first"}}"#,
    );

    let messages = parse_grok_unified_log_file(&path);

    assert_eq!(messages.len(), 2);
    assert_ne!(messages[0].dedup_key, messages[1].dedup_key);
    assert_eq!(messages[0].timestamp, messages[1].timestamp);
    assert_eq!(messages[0].tokens.total(), messages[1].tokens.total());
}

#[test]
fn unified_log_preserves_session_workspace_metadata() {
    let temp = tempfile::TempDir::new().unwrap();
    let logs_dir = temp.path().join("home/.grok/logs");
    let session_dir = temp
        .path()
        .join("home/.grok/sessions/%2Ftmp%2Fproject/session-1");
    std::fs::create_dir_all(&logs_dir).unwrap();
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
        session_dir.join("summary.json"),
        r#"{"current_model_id":"grok-4.5","updated_at":"2023-11-14T22:13:20Z"}"#,
    )
    .unwrap();
    let path = logs_dir.join("unified.jsonl");
    std::fs::write(
        &path,
        r#"{"ts":"2023-11-14T22:13:20Z","pid":17,"sid":"session-1","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":10,"cached_prompt_tokens":2,"completion_tokens":4,"reasoning_tokens":1}}"#,
    )
    .unwrap();

    let messages = parse_grok_unified_log_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "grok-4.5");
    assert_eq!(messages[0].workspace_key.as_deref(), Some("/tmp/project"));
    assert_eq!(messages[0].workspace_label.as_deref(), Some("project"));
}

#[test]
fn unified_log_applies_pidless_session_model_switch() {
    let (_temp, path) = write_unified_fixture(
        r#"{"ts":"2023-11-14T22:13:18Z","pid":17,"msg":"model catalog: notifying clients","ctx":{"current_model_id":"grok-4.5"}}
{"ts":"2023-11-14T22:13:19Z","pid":17,"sid":"session-with-model-event","msg":"model changed","ctx":{"model":"grok-composer-2.5-fast"}}
{"ts":"2023-11-14T22:13:20Z","pid":17,"sid":"session-with-model-event","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":10,"completion_tokens":1}}
{"ts":"2023-11-14T22:13:21Z","sid":"session-with-model-event","msg":"model changed","ctx":{"model":"grok-4.1-fast"}}
{"ts":"2023-11-14T22:13:22Z","pid":17,"sid":"session-with-model-event","msg":"shell.turn.inference_done","ctx":{"loop_index":2,"prompt_tokens":15,"completion_tokens":2}}
{"ts":"2023-11-14T22:13:23Z","pid":17,"sid":"session-without-model-event","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":20,"completion_tokens":2}}"#,
    );

    let messages = parse_grok_unified_log_file(&path);

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].model_id, "grok-composer-2.5-fast");
    assert_eq!(messages[1].model_id, "grok-4.1-fast");
    assert_eq!(messages[2].model_id, "grok-4.5");
}

#[test]
fn unified_log_expires_pid_scoped_models_on_process_restart() {
    let (_temp, path) = write_unified_fixture(
        r#"{"ts":"2023-11-14T22:13:17Z","sid":"session-stable","msg":"model changed","ctx":{"model":"grok-session"}}
{"ts":"2023-11-14T22:13:18Z","pid":17,"msg":"model catalog: notifying clients","ctx":{"current_model_id":"grok-old"}}
{"ts":"2023-11-14T22:13:19Z","pid":17,"sid":"session-old","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":10,"completion_tokens":1}}
{"ts":"2023-11-14T22:13:20Z","pid":17,"msg":"AuthManager::new","src":"shell","ctx":{}}
{"ts":"2023-11-14T22:13:21Z","pid":17,"sid":"session-stable","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":15,"completion_tokens":1}}
{"ts":"2023-11-14T22:13:22Z","pid":17,"sid":"session-new","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":20,"completion_tokens":2}}
{"ts":"2023-11-14T22:13:23Z","pid":17,"msg":"model catalog: notifying clients","ctx":{"current_model_id":"grok-new"}}
{"ts":"2023-11-14T22:13:24Z","pid":17,"sid":"session-new","msg":"shell.turn.inference_done","ctx":{"loop_index":2,"prompt_tokens":30,"completion_tokens":3}}"#,
    );

    let messages = parse_grok_unified_log_file(&path);

    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].model_id, "grok-old");
    assert_eq!(messages[1].model_id, "grok-session");
    assert_eq!(messages[2].model_id, UNKNOWN_MODEL);
    assert_eq!(messages[3].model_id, "grok-new");
}

#[test]
fn unified_log_attributes_parent_and_child_models_by_exact_scope() {
    let (_temp, path) = write_unified_fixture(
        r#"{"ts":"2026-07-31T00:00:00Z","pid":17,"msg":"subagent read parent config (live)","ctx":{"session_model_id":" grok-4.6 ","parent_model":"grok-4.5","global_model_id":"grok-4.4"}}
{"ts":"2026-07-31T00:00:01Z","pid":17,"sid":"parent","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":10,"completion_tokens":2}}
{"ts":"2026-07-31T00:00:02Z","pid":17,"msg":"subagent spawn credentials","ctx":{"subagent_id":"child-a","effective_model":" grok-4.7 ","effective_model_raw":"raw-a","parent_model":"grok-4.6"}}
{"ts":"2026-07-31T00:00:03Z","pid":17,"sid":"child-a","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":11,"completion_tokens":2}}
{"ts":"2026-07-31T00:00:04Z","pid":17,"msg":"subagent spawn credentials","ctx":{"subagent_id":"child-b","effective_model":"grok-4.8","parent_model":"grok-4.6"}}
{"ts":"2026-07-31T00:00:05Z","pid":17,"sid":"child-b","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":12,"completion_tokens":2}}
{"ts":"2026-07-31T00:00:06Z","sid":"child-a","msg":"model changed","ctx":{"model":"grok-global"}}
{"ts":"2026-07-31T00:00:07Z","pid":17,"sid":"child-a","msg":"shell.turn.inference_done","ctx":{"loop_index":2,"prompt_tokens":13,"completion_tokens":2}}
{"ts":"2026-07-31T00:00:08Z","sid":"ordinary","msg":"model changed","ctx":{"model":" grok-ordinary "}}
{"ts":"2026-07-31T00:00:09Z","pid":17,"sid":"ordinary","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":14,"completion_tokens":2}}"#,
    );

    let messages = parse_grok_unified_log_file(&path);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.model_id.as_str())
            .collect::<Vec<_>>(),
        [
            "grok-4.6",
            "grok-4.7",
            "grok-4.8",
            "grok-4.7",
            "grok-ordinary"
        ]
    );
}

#[test]
fn unified_log_fails_closed_on_conflicting_child_evidence() {
    let (_temp, path) = write_unified_fixture(
        r#"{"ts":"2026-07-31T00:00:00Z","pid":19,"sid":"child","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":10,"completion_tokens":2}}
{"ts":"2026-07-31T00:00:01Z","pid":19,"msg":"subagent spawn credentials","ctx":{"subagent_id":"child","effective_model":"grok-4.8"}}
{"ts":"2026-07-31T00:00:02Z","pid":19,"msg":"subagent failed","ctx":{"subagent_id":"child","effective_model":"grok-4.9"}}
{"ts":"2026-07-31T00:00:03Z","pid":19,"sid":"child","msg":"shell.turn.inference_done","ctx":{"loop_index":2,"prompt_tokens":11,"completion_tokens":2}}
{"ts":"2026-07-31T00:00:04Z","pid":19,"msg":"subagent completed","ctx":{"subagent_id":"missing","effective_model":null}}
{"ts":"2026-07-31T00:00:05Z","pid":19,"sid":"missing","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":12,"completion_tokens":2}}"#,
    );

    let messages = parse_grok_unified_log_file(&path);
    assert_eq!(messages.len(), 3);
    assert!(messages
        .iter()
        .all(|message| message.model_id == UNKNOWN_MODEL));
}

#[test]
fn unified_log_snapshot_ignores_rows_appended_after_scan_start() {
    use std::io::Write;

    let (_temp, path) = write_unified_fixture(
        r#"{"ts":"2026-07-31T00:00:00Z","pid":23,"sid":"first","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":10,"completion_tokens":2}}
"#,
    );
    let prefix_len = std::fs::metadata(&path).unwrap().len();
    std::fs::OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(
            br#"{"ts":"2026-07-31T00:00:01Z","pid":23,"sid":"second","msg":"shell.turn.inference_done","ctx":{"loop_index":1,"prompt_tokens":11,"completion_tokens":2}}
"#,
        )
        .unwrap();

    assert_eq!(
        parse_grok_unified_log_file_with_prefix(&path, prefix_len).len(),
        1
    );
    assert_eq!(parse_grok_unified_log_file(&path).len(), 2);
}
