use super::*;

#[test]
fn journal_update_for_snapshotted_id_wins_and_collapses_to_one_entry() {
    // The snapshot persists an in-flight assistant message with partial
    // token_usage; the next checkpoint hasn't rewritten the snapshot yet, so
    // the journal carries the SAME message_id with the final (larger)
    // token_usage. The journal value must win and the message_id must
    // collapse to exactly one entry (no double-counting).
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_live","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"append_messages":[{"id":"assistant_live","role":"assistant","timestamp":"2026-06-16T12:00:05Z","token_usage":{"input_tokens":900,"output_tokens":300,"cache_read_input_tokens":40}}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    // Exactly one entry for the repeated id (no double-counting).
    assert_eq!(messages.len(), 1);
    // Journal value wins over the stale snapshot value.
    assert_eq!(messages[0].tokens.input, 860);
    assert_eq!(messages[0].tokens.output, 300);
    assert_eq!(messages[0].tokens.cache_read, 40);
}

#[test]
fn journal_update_replaces_value_after_downstream_dedup() {
    // Mirror the lib.rs dedup contract: should_keep_deduped_message keeps the
    // FIRST occurrence per dedup_key. The in-parser merge must therefore have
    // already replaced the snapshot value in place, so the surviving entry
    // carries the journal's token_usage even after downstream dedup.
    use std::collections::HashSet;

    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_live","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"append_messages":[{"id":"assistant_live","role":"assistant","timestamp":"2026-06-16T12:00:05Z","token_usage":{"input_tokens":900,"output_tokens":300}}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    let mut seen: HashSet<String> = HashSet::new();
    let deduped: Vec<_> = messages
        .into_iter()
        .filter(|message| {
            message
                .dedup_key
                .as_ref()
                .is_none_or(|key| seen.insert(key.clone()))
        })
        .collect();
    assert_eq!(deduped.len(), 1);
    assert_eq!(deduped[0].tokens.input, 900);
    assert_eq!(deduped[0].tokens.output, 300);
}

#[test]
fn journal_correction_of_snapshot_message_keeps_pending_turn_start() {
    // The snapshot ends on a user message, so a turn-start is pending when
    // the journal is merged. The journal's first entry corrects an
    // already-snapshotted assistant id (a replace, whose is_turn_start is
    // taken from the snapshot during the merge), and its second entry opens
    // a brand-new assistant turn. The correction must stay turn-neutral: if
    // it consumes the pending turn-start, the following new assistant is
    // never marked is_turn_start and the session's turn_count is
    // under-counted by one.
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"user_1","role":"user","timestamp":"2026-06-16T12:00:00Z"},
{"id":"assistant_snap","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}},
{"id":"user_2","role":"user","timestamp":"2026-06-16T12:00:02Z"}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"append_messages":[{"id":"assistant_snap","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":150,"output_tokens":15}}]}
{"append_messages":[{"id":"assistant_journal","role":"assistant","timestamp":"2026-06-16T12:00:03Z","token_usage":{"input_tokens":200,"output_tokens":20}}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(messages.len(), 2);
    // The snapshot assistant keeps its turn-start; the journal correction
    // replaced its token_usage in place (150 in), preserving the flag.
    assert!(messages[0].is_turn_start);
    assert_eq!(messages[0].tokens.input, 150);
    // The brand-new journal assistant opens the second turn.
    assert!(messages[1].is_turn_start);
    let turn_count = messages.iter().filter(|m| m.is_turn_start).count();
    assert_eq!(turn_count, 2);
}

#[test]
fn journal_full_turn_replay_does_not_double_count_turns() {
    // The journal replays a whole already-snapshotted turn (user + assistant
    // correction), then appends a follow-up assistant step of the SAME turn.
    // The user replay must not re-arm pending_turn_start: assistant ids are
    // guarded via known_dedup_keys, but user messages never enter the index
    // (no usage), so their replay is indistinguishable from a new turn.
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"user_1","role":"user","timestamp":"2026-06-16T12:00:00Z"},
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();
    std::fs::write(
        dir.path().join("session_test.journal.jsonl"),
        r#"{"append_messages":[{"id":"user_1","role":"user","timestamp":"2026-06-16T12:00:00Z"},{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:01Z","token_usage":{"input_tokens":150,"output_tokens":15}}]}
{"append_messages":[{"id":"assistant_1b","role":"assistant","timestamp":"2026-06-16T12:00:04Z","token_usage":{"input_tokens":50,"output_tokens":5}}]}
"#,
    )
    .unwrap();

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(messages.len(), 2);
    let turn_count = messages.iter().filter(|m| m.is_turn_start).count();
    assert_eq!(
        turn_count, 1,
        "a replayed user message must not mint a second turn"
    );
}
