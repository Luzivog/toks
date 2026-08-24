use super::*;

#[test]
fn test_parse_kiro_global_storage_dedup_keys_differ_across_workspaces() {
    let dir = TempDir::new().unwrap();
    let payload = r#"{
                "model": "auto",
                "messages": [
                    {"role": "user", "content": "hello world"},
                    {"role": "assistant", "content": "response text"}
                ]
            }"#;

    // Two `execution.chat` snapshots under DIFFERENT workspaces.
    let path_a = dir.path().join(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/workspace-a/execution.chat",
        );
    let path_b = dir.path().join(
            "Library/Application Support/Kiro/User/globalStorage/kiro.kiroagent/workspace-b/execution.chat",
        );
    fs::create_dir_all(path_a.parent().unwrap()).unwrap();
    fs::create_dir_all(path_b.parent().unwrap()).unwrap();
    fs::write(&path_a, payload).unwrap();
    fs::write(&path_b, payload).unwrap();

    let messages_a = parse_kiro_file(&path_a);
    let messages_b = parse_kiro_file(&path_b);

    assert_eq!(messages_a.len(), 1);
    assert_eq!(messages_b.len(), 1);
    assert_ne!(messages_a[0].dedup_key, messages_b[0].dedup_key);
    assert_eq!(
        messages_a[0].dedup_key,
        Some("workspace-a/execution:globalstorage".to_string())
    );
    assert_eq!(
        messages_b[0].dedup_key,
        Some("workspace-b/execution:globalstorage".to_string())
    );
}

#[test]
fn test_parse_kiro_global_storage_ignores_unknown_roles() {
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
                    {"role": "mystery", "content": "mystery text"},
                    {"role": "assistant", "content": "response text"}
                ]
            }"#,
    )
    .unwrap();

    let messages = parse_kiro_file(&file_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.output, 4);
}

#[test]
fn test_collect_kiro_snapshot_text_does_not_double_count_aliased_keys() {
    // (a) A single message object that stores the SAME assistant body under
    // two aliased text keys (`content` and `text`). Before the fix, the
    // traversal descended into every present alias and counted "response
    // text" twice (8 assistant chars -> output 2). After the fix it descends
    // into only the first present alias in the group, counting once.
    let value: Value = serde_json::from_str(
        r#"{
                "messages": [
                    {"role": "assistant", "content": "abcd", "text": "abcd"}
                ]
            }"#,
    )
    .unwrap();

    let mut counts = KiroSnapshotTextCounts::default();
    collect_kiro_snapshot_text(&value, &mut counts, None);

    // "abcd" counted once = 4 chars, not 8.
    assert_eq!(counts.assistant_chars, 4);
    assert_eq!(counts.prompt_chars, 0);
}

#[test]
fn test_collect_kiro_snapshot_text_does_not_double_count_aliased_containers() {
    // (a) An object that stores the SAME conversation list under two aliased
    // container keys (`messages` and `entries`). Before the fix both were
    // traversed and the text was counted twice.
    let value: Value = serde_json::from_str(
        r#"{
                "messages": [{"role": "user", "content": "hello"}],
                "entries": [{"role": "user", "content": "hello"}]
            }"#,
    )
    .unwrap();

    let mut counts = KiroSnapshotTextCounts::default();
    collect_kiro_snapshot_text(&value, &mut counts, None);

    // "hello" counted once = 5 chars, not 10.
    assert_eq!(counts.prompt_chars, 5);
    assert_eq!(counts.assistant_chars, 0);
}

#[test]
fn test_collect_kiro_snapshot_text_counts_distinct_alias_subtrees() {
    // A single turn that stores DISTINCT payloads under two keys of the same
    // alias group: `prompt` (user text) and `response` (assistant text).
    // These are different subtrees, so both must be counted. A first-key-only
    // traversal would drop the `response` body and undercount.
    let value: Value = serde_json::from_str(
        r#"{
                "prompt": {"role": "user", "text": "hi there"},
                "response": {"role": "assistant", "text": "hello back"}
            }"#,
    )
    .unwrap();

    let mut counts = KiroSnapshotTextCounts::default();
    collect_kiro_snapshot_text(&value, &mut counts, None);

    // "hi there" = 8 prompt chars, "hello back" = 10 assistant chars.
    assert_eq!(counts.prompt_chars, 8);
    assert_eq!(counts.assistant_chars, 10);
}

#[test]
fn test_collect_kiro_snapshot_text_counts_distinct_container_subtrees() {
    // A chat object holding DISTINCT conversation lists under two container
    // aliases (`messages` and `history`). Both must be counted; the
    // value-based de-dup only skips structurally identical subtrees.
    let value: Value = serde_json::from_str(
        r#"{
                "messages": [{"role": "user", "content": "alpha"}],
                "history": [{"role": "user", "content": "bravo"}]
            }"#,
    )
    .unwrap();

    let mut counts = KiroSnapshotTextCounts::default();
    collect_kiro_snapshot_text(&value, &mut counts, None);

    // "alpha" (5) + "bravo" (5) = 10 prompt chars; nothing dropped.
    assert_eq!(counts.prompt_chars, 10);
    assert_eq!(counts.assistant_chars, 0);
}

#[test]
fn test_find_kiro_snapshot_model_id_descends_into_aliased_text_keys() {
    // (b) Model id nested under `parts` / `prompt` — keys that
    // `collect_kiro_snapshot_text` descends into but the model-id finder
    // previously omitted, causing the model to fall back to `unknown`.
    let parts_value: Value = serde_json::from_str(
        r#"{
                "messages": [
                    {"parts": [{"model_id": "claude-sonnet-4-5"}]}
                ]
            }"#,
    )
    .unwrap();
    assert_eq!(
        find_kiro_snapshot_model_id(&parts_value),
        Some("claude-sonnet-4-5".to_string())
    );

    let prompt_value: Value =
        serde_json::from_str(r#"{"prompt": {"model": "claude-sonnet-4"}}"#).unwrap();
    assert_eq!(
        find_kiro_snapshot_model_id(&prompt_value),
        Some("claude-sonnet-4".to_string())
    );
}
