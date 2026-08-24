use super::*;

#[test]
fn parses_jcode_journal_append_messages() {
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "provider_key":"cliproxyapi",
  "model":"snapshot-model",
  "working_dir":"/Users/alice/project",
  "messages":[
{"id":"assistant_snapshot","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"meta":{"provider_key":"openai","model":"journal-model","working_dir":"/Users/alice/journal-project"},"append_messages":[{"id":"assistant_journal","role":"assistant","timestamp":"2026-06-16T12:00:02Z","token_usage":{"input_tokens":200,"output_tokens":20,"cache_read_input_tokens":50}}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].model_id, "snapshot-model");
    assert_eq!(messages[0].provider_id, "cliproxyapi");
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[1].model_id, "journal-model");
    assert_eq!(messages[1].provider_id, "openai");
    assert_eq!(messages[1].tokens.input, 150);
    assert_eq!(messages[1].tokens.cache_read, 50);
    assert_eq!(
        messages[1].workspace_label.as_deref(),
        Some("journal-project")
    );
}

#[test]
fn uses_journal_mtime_for_journal_messages_without_timestamps() {
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    let journal = dir.path().join("session_test.journal.jsonl");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_snapshot","role":"assistant","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        &journal,
        r#"{"append_messages":[{"id":"assistant_journal","role":"assistant","token_usage":{"input_tokens":200,"output_tokens":20}}]}
"#,
    )
    .unwrap();

    let snapshot_time =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000);
    let journal_time =
        std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_700_086_400);
    let snapshot_file = std::fs::OpenOptions::new()
        .write(true)
        .open(&snapshot)
        .unwrap();
    let Ok(()) = snapshot_file.set_modified(snapshot_time) else {
        return;
    };
    drop(snapshot_file);
    let journal_file = std::fs::OpenOptions::new()
        .write(true)
        .open(&journal)
        .unwrap();
    let Ok(()) = journal_file.set_modified(journal_time) else {
        return;
    };
    drop(journal_file);

    let snapshot_fallback = file_modified_timestamp_ms(&snapshot);
    let journal_fallback = file_modified_timestamp_ms(&journal);
    assert_ne!(snapshot_fallback, journal_fallback);

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].timestamp, snapshot_fallback);
    assert_eq!(messages[1].timestamp, journal_fallback);
}

#[test]
fn skips_unreadable_journal_lines_and_continues_parsing() {
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[]
}"#,
    )
    .unwrap();

    let mut journal = Vec::new();
    journal.extend_from_slice(
        br#"{"append_messages":[{"id":"assistant_before","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}}]}
"#,
    );
    journal.extend_from_slice(b"\xff\n");
    journal.extend_from_slice(
        br#"{"append_messages":[{"id":"assistant_after","role":"assistant","timestamp":"2026-06-16T12:00:02Z","token_usage":{"input_tokens":200,"output_tokens":20}}]}
"#,
    );
    std::fs::write(dir.path().join("session_test.journal.jsonl"), journal).unwrap();

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[1].tokens.input, 200);
}

#[test]
fn one_malformed_journal_sibling_does_not_drop_the_lines_valid_messages() {
    // Same leniency as the snapshot: a malformed sibling inside a journal
    // line's append_messages batch must only drop that element, not the
    // valid messages (or meta) sharing the line.
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_good","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"append_messages":[{"id":"assistant_journal","role":"assistant","timestamp":"2026-06-16T12:00:03Z","token_usage":{"input_tokens":200,"output_tokens":20}},{"id":"assistant_bad","role":"assistant","timestamp":"2026-06-16T12:00:04Z","token_usage":"corrupt"}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(
        messages.len(),
        2,
        "a valid journal message must survive a malformed sibling on its line"
    );
    let total_input: i64 = messages.iter().map(|m| m.tokens.input).sum();
    assert_eq!(total_input, 300);
}
