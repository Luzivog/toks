use super::*;

#[test]
fn parses_timezone_less_timestamps_instead_of_falling_back_to_mtime() {
    // Jcode (and proxy variants) sometimes emit naive ISO-8601 datetimes
    // with no `Z`/offset. These must parse as UTC, not collapse to the
    // file mtime (which would scatter the message into the wrong bucket).
    let dir = tempfile::TempDir::new().unwrap();
    let snapshot = dir.path().join("session_test.json");
    std::fs::write(
        &snapshot,
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_naive","role":"assistant","timestamp":"2026-06-16T12:00:00","token_usage":{"input_tokens":100,"output_tokens":10}}
  ]
}"#,
    )
    .unwrap();

    // Force a clearly-different mtime so a fallback would be detectable.
    let mtime = std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
    if let Ok(file) = std::fs::OpenOptions::new().write(true).open(&snapshot) {
        let _ = file.set_modified(mtime);
    }
    let fallback = file_modified_timestamp_ms(&snapshot);

    let messages = parse_jcode_file(&snapshot);
    assert_eq!(messages.len(), 1);
    // "2026-06-16T12:00:00" UTC == 1781611200000 ms.
    assert_eq!(messages[0].timestamp, 1_781_611_200_000);
    assert_ne!(messages[0].timestamp, fallback);
}

#[test]
fn test_tool_duration_timestamp_is_start_anchored() {
    // Regression (follow-up to #890): an assistant message's `timestamp`
    // is written once the message (including `token_usage`) is
    // finalized, i.e. the turn's *end*, not its start. `tool_duration_ms`
    // is that turn's elapsed time, so sessionize()'s
    // `[timestamp, timestamp + duration_ms]` span would otherwise project
    // forward past the actual completion into phantom idle time. The
    // parser must back-calculate the start anchor instead.
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(
        file.path(),
        r#"{
  "id":"session_test",
  "model":"snapshot-model",
  "messages":[
{"id":"assistant_1","role":"assistant","timestamp":"2026-06-16T12:00:05Z","token_usage":{"input_tokens":100,"output_tokens":10},"tool_duration_ms":2000}
  ]
}"#,
    )
    .unwrap();

    let messages = parse_jcode_file(file.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].timestamp,
        parse_timestamp_str("2026-06-16T12:00:03Z").unwrap(),
        "timestamp must be back-calculated to the turn start (end - duration)"
    );
    assert_eq!(
        messages[0].duration_ms,
        Some(2000),
        "duration_ms must still span from start to the recorded end timestamp"
    );
}
