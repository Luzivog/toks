use super::*;

#[test]
fn test_parse_kiro_global_storage_chat_file() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/workspace-a/execution.chat",
        );
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
        &file_path,
        r#"{
                "model": "auto",
                "messages": [
                    {"role": "user", "content": "hello world"},
                    {"role": "assistant", "content": "response text"}
                ]
            }"#,
    )
    .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "kiro");
    assert_eq!(messages[0].model_id, "auto");
    assert!(messages[0].tokens.input > 0);
    assert!(messages[0].tokens.output > 0);
    // (4a) Workspace attribution: the `<workspace>` segment after
    // `kiro.kiroagent/` flows through the same workspace helpers.
    assert_eq!(messages[0].workspace_key, Some("workspace-a".to_string()));
    assert_eq!(messages[0].workspace_label, Some("workspace-a".to_string()));
    assert_eq!(
        messages[0].dedup_key,
        Some("workspace-a/execution:globalstorage".to_string())
    );
}

#[test]
fn test_parse_kiro_execution_file_attributes_workspace_model_and_duration() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/workspace-a/execution-123.json",
        );
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
            &file_path,
            r#"{
                "executionId": "exec-123",
                "chatSessionId": "chat-abc",
                "status": "succeed",
                "startTime": 1770983426000,
                "endTime": 1770983427500,
                "completionOptions": {"modelId": "claude-sonnet-4-5"},
                "actions": [
                    {"actionType": "say", "output": "the assistant replied with a full answer"},
                    {"actionType": "reasoning", "output": {"message": "thinking it through"}}
                ],
                "context": {
                    "messages": [
                        {"entries": [{"type": "text", "text": "user asks a reasonably long question"}]}
                    ]
                }
            }"#,
        )
        .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "chat-abc");
    assert_eq!(
        messages[0].dedup_key,
        Some("execution:exec-123".to_string())
    );
    assert!(messages[0].tokens.input > 0);
    assert!(messages[0].tokens.output > 0);
    // Model is extracted from completionOptions, not hardcoded to "auto".
    assert_eq!(messages[0].model_id, "claude-sonnet-4-5");
    // Workspace attribution matches the snapshot path.
    assert_eq!(messages[0].workspace_key, Some("workspace-a".to_string()));
    assert_eq!(messages[0].workspace_label, Some("workspace-a".to_string()));
    // Duration is carried through (endTime - startTime = 1500ms).
    assert_eq!(messages[0].duration_ms, Some(1500));
}

#[test]
fn test_parse_kiro_execution_file_parses_seconds_epoch_start_time() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/workspace-a/execution-secs.json",
        );
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    // startTime as an epoch-seconds integer must be scaled to ms, not read
    // as a millisecond value (which would file it under 1970).
    fs::write(
        &file_path,
        r#"{
                "executionId": "exec-secs",
                "status": "succeed",
                "startTime": 1770983426,
                "actions": [{"actionType": "say", "output": "answer text here"}],
                "context": {
                    "messages": [
                        {"entries": [{"type": "text", "text": "a question from the user"}]}
                    ]
                }
            }"#,
    )
    .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    // 1770983426 seconds -> 1770983426000 ms -> 2026, not 1970.
    assert_eq!(messages[0].timestamp, 1770983426000);
    assert!(messages[0].date.starts_with("2026-"));
}

#[test]
fn suppress_snapshots_covered_by_executions_drops_only_exact_matches() {
    let messages = vec![
        // Snapshot for chat-abc in workspace-a: covered by the execution below.
        make_globalstorage_message(
            "workspace-a/chat-abc",
            "workspace-a/chat-abc:globalstorage",
            Some("workspace-a"),
        ),
        // Execution for the same chat session and workspace.
        make_globalstorage_message("chat-abc", "execution:exec-1", Some("workspace-a")),
        // Snapshot with a different stem: kept.
        make_globalstorage_message(
            "workspace-a/other-session",
            "workspace-a/other-session:globalstorage",
            Some("workspace-a"),
        ),
        // Same stem but different workspace: kept.
        make_globalstorage_message(
            "workspace-b/chat-abc",
            "workspace-b/chat-abc:globalstorage",
            Some("workspace-b"),
        ),
    ];

    let kept = suppress_snapshots_covered_by_executions(messages);

    let keys: Vec<&str> = kept
        .iter()
        .filter_map(|message| message.dedup_key.as_deref())
        .collect();
    assert_eq!(kept.len(), 3);
    assert!(keys.contains(&"execution:exec-1"));
    assert!(keys.contains(&"workspace-a/other-session:globalstorage"));
    assert!(keys.contains(&"workspace-b/chat-abc:globalstorage"));
    assert!(!keys.contains(&"workspace-a/chat-abc:globalstorage"));
}

#[test]
fn test_parse_kiro_workspace_session_promptlogs() {
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/workspace-sessions/d29ya3NwYWNl/sess-uuid-1.json",
        );
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
            &file_path,
            r#"{
                "sessionId": "sess-uuid-1",
                "selectedModel": "claude-sonnet-4",
                "history": [
                    {
                        "message": {"role": "user", "content": "hello"},
                        "promptLogs": [{"prompt": "0123456789012345", "completion": "hi"}]
                    },
                    {
                        "message": {"role": "assistant", "content": "On it."},
                        "promptLogs": [{"prompt": "01234567890123456789012345678901", "completion": "done"}]
                    }
                ]
            }"#,
        )
        .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "sess-uuid-1");
    assert_eq!(messages[0].model_id, "claude-sonnet-4");
    // 16 + 32 prompt chars -> ceil(48 / 4) = 12 estimated input tokens.
    assert_eq!(messages[0].tokens.input, 12);
    // "On it." -> ceil(6 / 4) = 2 estimated output tokens.
    assert_eq!(messages[0].tokens.output, 2);
    assert_eq!(messages[0].message_count, 2);
    assert_eq!(
        messages[0].dedup_key,
        Some("sess-uuid-1:workspace-session".to_string())
    );
}

#[test]
fn suppress_drops_workspace_session_covered_by_execution() {
    let messages = vec![
        // Workspace-session promptLogs snapshot for sess-1: covered by the
        // execution below even though the workspace keys differ (the two
        // stores live under different kiro.kiroagent subdirectories).
        make_globalstorage_message(
            "sess-1",
            "sess-1:workspace-session",
            Some("workspace-sessions"),
        ),
        // Execution whose chatSessionId is the same session UUID.
        make_globalstorage_message("sess-1", "execution:exec-9", Some("abc080c47e826767")),
        // Workspace-session for a session with no counted execution: kept.
        make_globalstorage_message(
            "sess-2",
            "sess-2:workspace-session",
            Some("workspace-sessions"),
        ),
    ];

    let kept = suppress_snapshots_covered_by_executions(messages);

    let keys: Vec<&str> = kept
        .iter()
        .filter_map(|message| message.dedup_key.as_deref())
        .collect();
    assert_eq!(kept.len(), 2);
    assert!(keys.contains(&"execution:exec-9"));
    assert!(keys.contains(&"sess-2:workspace-session"));
    assert!(!keys.contains(&"sess-1:workspace-session"));
}

#[test]
fn suppress_snapshots_is_noop_without_executions() {
    let messages = vec![make_globalstorage_message(
        "workspace-a/chat-abc",
        "workspace-a/chat-abc:globalstorage",
        Some("workspace-a"),
    )];

    let kept = suppress_snapshots_covered_by_executions(messages);
    assert_eq!(kept.len(), 1);
}

#[test]
fn parse_kiro_chat_artifact_counts_human_and_bot_roles() {
    // Real IDE-private .chat files use human/bot/tool roles; tool context
    // is intentionally not counted.
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(
        "Kiro/User/globalStorage/kiro.kiroagent/workspace-a/0c433dc89e4c1803dd6fe838634ed7fc.chat",
    );
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
        &file_path,
        r#"{
                "executionId": "5b40545a-2539-4334-9411-23df0bfea51b",
                "actionId": "act",
                "chat": [
                    {"role": "human", "content": "please refactor the loader"},
                    {"role": "tool", "content": "You are operating in a workspace"},
                    {"role": "bot", "content": "Done, refactored."}
                ],
                "metadata": {}
            }"#,
    )
    .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    // human: 26 chars -> ceil(26/4) = 7; bot: 17 chars -> ceil(17/4) = 5.
    // The 32-char tool line is excluded from both.
    assert_eq!(messages[0].tokens.input, 7);
    assert_eq!(messages[0].tokens.output, 5);
}

#[test]
fn parse_kiro_chat_artifact_tags_dedup_key_with_execution_id() {
    // Shape observed in real globalStorage trees: `<hash>.chat` carries a
    // top-level executionId (and NO `actions`, so it must not be parsed as
    // an execution record).
    let dir = TempDir::new().unwrap();
    let file_path = dir.path().join(
            "Kiro/User/globalStorage/kiro.kiroagent/abc080c47e826767f65b27677d791c66/006924fffc3bc58648f10379cdfd77a6.chat",
        );
    fs::create_dir_all(file_path.parent().unwrap()).unwrap();
    fs::write(
        &file_path,
        r#"{
                "executionId": "3067e447-2cda-47c9-a476-536a72d92f31",
                "actionId": "act",
                "context": {},
                "chat": [
                    {"role": "user", "content": "please refactor the config loader"},
                    {"role": "assistant", "content": "On it."}
                ],
                "metadata": {"workflowId": "3e445aa7-f59c-4bf4-a471-c655dad734f5"}
            }"#,
    )
    .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(
            messages[0].dedup_key.as_deref(),
            Some(
                "abc080c47e826767f65b27677d791c66/006924fffc3bc58648f10379cdfd77a6:globalstorage:exec:3067e447-2cda-47c9-a476-536a72d92f31"
            )
        );
}

#[test]
fn suppress_snapshots_drops_chat_artifacts_matching_execution_id() {
    // Real-world id shapes: `.chat` stems are opaque 32-hex hashes, while
    // executionId/chatSessionId are dashed UUIDs — so only the executionId
    // tag can link the two.
    let ws = "abc080c47e826767f65b27677d791c66";
    let messages = vec![
            // Two .chat artifacts for the same execution: both covered.
            make_globalstorage_message(
                "abc080c47e826767f65b27677d791c66/006924fffc3bc58648f10379cdfd77a6",
                "abc080c47e826767f65b27677d791c66/006924fffc3bc58648f10379cdfd77a6:globalstorage:exec:3067e447-2cda-47c9-a476-536a72d92f31",
                Some(ws),
            ),
            make_globalstorage_message(
                "abc080c47e826767f65b27677d791c66/01e341965ac1caf00a9ecb9cc1635d62",
                "abc080c47e826767f65b27677d791c66/01e341965ac1caf00a9ecb9cc1635d62:globalstorage:exec:3067e447-2cda-47c9-a476-536a72d92f31",
                Some(ws),
            ),
            // The execution record itself (session id = chatSessionId).
            make_globalstorage_message(
                "efddf80a-eab9-4f1c-8a13-877eaac72736",
                "execution:3067e447-2cda-47c9-a476-536a72d92f31",
                Some(ws),
            ),
            // .chat artifact for an execution that is NOT counted (e.g. failed):
            // kept.
            make_globalstorage_message(
                "abc080c47e826767f65b27677d791c66/0681d950923f98601e198293ca2040fd",
                "abc080c47e826767f65b27677d791c66/0681d950923f98601e198293ca2040fd:globalstorage:exec:5b40545a-2539-4334-9411-23df0bfea51b",
                Some(ws),
            ),
            // Same execution id but a different workspace: kept.
            make_globalstorage_message(
                "other-ws/aaaa",
                "other-ws/aaaa:globalstorage:exec:3067e447-2cda-47c9-a476-536a72d92f31",
                Some("other-ws"),
            ),
        ];

    let kept = suppress_snapshots_covered_by_executions(messages);

    let keys: Vec<&str> = kept
        .iter()
        .filter_map(|message| message.dedup_key.as_deref())
        .collect();
    assert_eq!(kept.len(), 3);
    assert!(keys.contains(&"execution:3067e447-2cda-47c9-a476-536a72d92f31"));
    assert!(keys.iter().any(|key| key.contains("0681d950")));
    assert!(keys.iter().any(|key| key.starts_with("other-ws/aaaa")));
    assert!(!keys.iter().any(|key| key.contains("006924ff")));
    assert!(!keys.iter().any(|key| key.contains("01e34196")));
}
