use super::*;

#[test]
fn test_parse_kiro_ide_session_estimates_tokens_from_messages_jsonl() {
    // session.json is the schemaVersion 1.0.0 sample from issue #813.
    let session_json = r#"{
            "schemaVersion": "1.0.0",
            "dataModelVersion": 1,
            "id": "sess_02f1c107-37e8-4398-8b95-c3847bf59335",
            "title": "Writing README docs for projects",
            "agentMode": "vibe",
            "createdAt": "2026-06-30T12:57:10.991Z",
            "lastModifiedAt": "2026-06-30T12:57:12.991Z",
            "status": "completed"
        }"#;
    let messages_jsonl = "{\"role\":\"user\",\"content\":\"hello world\"}\n{\"role\":\"assistant\",\"content\":\"response text\"}\n";

    let dir = TempDir::new().unwrap();
    let path = create_ide_session_files(
        &dir,
        "my-project",
        "sess_02f1c107-37e8-4398-8b95-c3847bf59335",
        session_json,
        messages_jsonl,
    );

    let messages = parse_kiro_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "kiro");
    assert_eq!(messages[0].provider_id, "amazon-bedrock");
    assert_eq!(
        messages[0].session_id,
        "sess_02f1c107-37e8-4398-8b95-c3847bf59335"
    );
    // "hello world" = 11 chars -> ceil(11/4) = 3; "response text" = 13 -> 4.
    assert_eq!(messages[0].tokens.input, 3);
    assert_eq!(messages[0].tokens.output, 4);
    assert!(messages[0].is_turn_start);
    // Workspace is the folder holding the sess_* dir.
    assert_eq!(messages[0].workspace_key, Some("my-project".to_string()));
    assert_eq!(messages[0].workspace_label, Some("my-project".to_string()));
    // createdAt -> ms; duration = lastModifiedAt - createdAt = 2000ms.
    assert_eq!(messages[0].timestamp, 1782824230991);
    assert_eq!(messages[0].duration_ms, Some(2000));
    assert!(messages[0].date.starts_with("2026-"));
    // No model in session.json/messages.jsonl -> "auto" so pricing can resolve.
    assert_eq!(messages[0].model_id, "auto");
    // Dedup key is IDE-session-scoped and survives execution suppression.
    assert_eq!(
        messages[0].dedup_key,
        Some("sess_02f1c107-37e8-4398-8b95-c3847bf59335:ide-session".to_string())
    );
    // One assistant response -> message_count 1.
    assert_eq!(messages[0].message_count, 1);
}

#[test]
fn test_parse_kiro_ide_session_extracts_model_and_counts_turns() {
    let session_json = r#"{
            "schemaVersion": "1.0.0",
            "id": "sess_abc",
            "createdAt": "2026-06-30T12:57:10.000Z",
            "lastModifiedAt": "2026-06-30T12:57:10.000Z"
        }"#;
    // Two assistant turns and a Kiro IDE model codename embedded in a line.
    let messages_jsonl = concat!(
        "{\"role\":\"user\",\"content\":\"first question here\"}\n",
        "{\"role\":\"assistant\",\"model\":\"big-pickle\",\"content\":\"first answer\"}\n",
        "{\"role\":\"user\",\"content\":\"second question\"}\n",
        "{\"role\":\"assistant\",\"content\":\"second answer\"}\n"
    );

    let dir = TempDir::new().unwrap();
    let path = create_ide_session_files(&dir, "ws", "sess_abc", session_json, messages_jsonl);

    let messages = parse_kiro_file(&path);

    assert_eq!(messages.len(), 1);
    // Model codename is picked up from messages.jsonl (not a Kiro-internal id).
    assert_eq!(messages[0].model_id, "big-pickle");
    // Two assistant responses -> message_count 2.
    assert_eq!(messages[0].message_count, 2);
    assert!(messages[0].tokens.input > 0);
    assert!(messages[0].tokens.output > 0);
}

#[test]
fn test_structured_turn_missing_prompt_timestamp_back_calculates_from_elapsed_time() {
    // Second-round review fix: in the structured `messages.jsonl` layout,
    // `usage_summary.elapsedTime` can supply `duration_ms` while the user
    // prompt's own timestamp is absent (or unparseable). Previously the
    // message timestamp fell back to the `turn_end` event's own
    // timestamp, leaving the message end-anchored — sessionize()'s
    // `[timestamp, timestamp + duration_ms]` span would then project
    // forward past the turn's actual end into phantom idle time. The
    // parser must back-calculate `turn_end - elapsedTime` as the anchor
    // instead.
    let session_json = r#"{
            "schemaVersion": "1.0.0",
            "id": "sess_structured"
        }"#;
    let messages_jsonl = concat!(
        "{\"payload\":{\"type\":\"user\",\"content\":\"hello world\"}}\n",
        "{\"payload\":{\"type\":\"assistant\",\"content\":\"response text\"}}\n",
        "{\"payload\":{\"type\":\"usage_summary\",\"elapsedTime\":5000}}\n",
        "{\"payload\":{\"type\":\"turn_end\"},\"timestamp\":\"2026-06-20T10:00:05Z\"}\n",
    );

    let dir = TempDir::new().unwrap();
    let path =
        create_ide_session_files(&dir, "ws", "sess_structured", session_json, messages_jsonl);

    let messages = parse_kiro_file(&path);

    assert_eq!(messages.len(), 1);
    let expected_end = chrono::DateTime::parse_from_rfc3339("2026-06-20T10:00:05Z")
        .unwrap()
        .timestamp_millis();
    assert_eq!(
            messages[0].timestamp,
            expected_end - 5000,
            "timestamp must be back-calculated from turn_end - elapsedTime when the prompt timestamp is missing"
        );
    assert_eq!(messages[0].duration_ms, Some(5000));
}

#[test]
fn test_parse_kiro_ide_session_dropped_when_no_recognizable_content() {
    let session_json = r#"{"schemaVersion":"1.0.0","id":"sess_empty"}"#;
    // Only tool/system noise with no role-tagged conversation text.
    let messages_jsonl = "{\"kind\":\"toolCall\",\"name\":\"read_file\"}\n";

    let dir = TempDir::new().unwrap();
    let path = create_ide_session_files(&dir, "ws", "sess_empty", session_json, messages_jsonl);

    let messages = parse_kiro_file(&path);

    // No estimable usage -> no fabricated message.
    assert!(messages.is_empty());
}

#[test]
fn test_parse_kiro_ide_session_falls_back_to_dir_name_and_mtime() {
    // session.json with no id and no timestamps: session id falls back to the
    // sess_* directory name and timestamp to the file mtime.
    let session_json = r#"{"schemaVersion":"1.0.0"}"#;
    let messages_jsonl = "{\"role\":\"user\",\"content\":\"hello\"}\n";

    let dir = TempDir::new().unwrap();
    let path = create_ide_session_files(&dir, "ws", "sess_no_meta", session_json, messages_jsonl);

    let messages = parse_kiro_file(&path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "sess_no_meta");
    assert_eq!(
        messages[0].dedup_key,
        Some("sess_no_meta:ide-session".to_string())
    );
    assert!(messages[0].timestamp > 0);
}

#[test]
fn suppress_snapshots_leaves_ide_sessions_untouched() {
    // An IDE-session message must never be dropped by execution suppression,
    // even when a globalStorage execution is present in the same batch.
    let messages = vec![
        make_globalstorage_message("chat-abc", "execution:exec-1", Some("ws")),
        make_globalstorage_message("sess_abc", "sess_abc:ide-session", Some("ws")),
    ];

    let kept = suppress_snapshots_covered_by_executions(messages);

    let keys: Vec<&str> = kept
        .iter()
        .filter_map(|message| message.dedup_key.as_deref())
        .collect();
    assert_eq!(kept.len(), 2);
    assert!(keys.contains(&"sess_abc:ide-session"));
    assert!(keys.contains(&"execution:exec-1"));
}
