use super::*;

#[test]
fn parse_copilot_desktop_db_reads_token_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 100, 50, 25, 10);
    drop(conn);

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "copilot");
    assert_eq!(message.model_id, "gpt-5.1-codex");
    assert_eq!(message.provider_id, "openai");
    assert_eq!(message.session_id, "session-1");
    assert_eq!(message.timestamp, 1_782_909_296_000);
    // total_input_tokens is inclusive of cache reads, so the cached portion
    // (25) is normalized out of input: 100 - 25 = 75.
    assert_eq!(message.tokens.input, 75);
    assert_eq!(message.tokens.output, 50);
    assert_eq!(message.tokens.cache_read, 25);
    assert_eq!(message.tokens.cache_write, 0);
    assert_eq!(message.tokens.reasoning, 10);
    assert_eq!(
        message.dedup_key.as_deref(),
        Some("copilot-desktop:session-1")
    );
}

#[test]
fn parse_copilot_desktop_db_skips_zero_token_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "gpt-5.1-codex", 0, 0, 0, 0);
    drop(conn);

    assert!(parse_copilot_desktop_db(&db_path).is_empty());
}

#[test]
fn parse_copilot_desktop_db_enriches_model_and_workspace_from_events() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "auto", 100, 50, 0, 0);
    drop(conn);
    write_events(
        dir.path(),
        "session-1",
        &[
            r#"{"type":"session.start","data":{"context":{"cwd":"/Users/alice/project"}}}"#,
            r#"{"type":"session.model_change","data":{"newModel":"claude-sonnet-4-5"}}"#,
        ],
    );

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.model_id, "claude-sonnet-4-5");
    assert_eq!(message.provider_id, "anthropic");
    assert_eq!(message.workspace_label.as_deref(), Some("project"));
}

#[test]
fn keeps_reading_events_after_an_undecodable_line() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "auto", 100, 50, 0, 0);
    drop(conn);

    let events_dir = dir.path().join("session-state").join("session-1");
    fs::create_dir_all(&events_dir).unwrap();
    let mut fixture = Vec::new();
    fixture.extend_from_slice(
        br#"{"type":"session.start","data":{"context":{"cwd":"/Users/alice/project"}}}"#,
    );
    fixture.push(b'\n');
    // A lone 0xff can never appear in valid UTF-8, so `BufRead::lines()`
    // reports this line as `InvalidData` and `map_while(Result::ok)` would
    // treat it as end of file, losing the model change below it.
    fixture.extend_from_slice(b"{\"type\":\"session.note\",\"data\":\"\xff\xfe\"}\n");
    fixture.extend_from_slice(
        br#"{"type":"session.model_change","data":{"newModel":"claude-sonnet-4-5"}}"#,
    );
    fixture.push(b'\n');
    fs::write(events_dir.join("events.jsonl"), &fixture).unwrap();

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "claude-sonnet-4-5");
    assert_eq!(messages[0].provider_id, "anthropic");
}

#[test]
fn parse_copilot_desktop_db_uses_github_copilot_provider_for_auto() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("data.db");
    let conn = create_copilot_desktop_db(&db_path);
    insert_session(&conn, "session-1", "auto", 100, 0, 0, 0);
    drop(conn);

    let messages = parse_copilot_desktop_db(&db_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].provider_id, "github-copilot");
}

/// Regression (#962): the row carries a lifetime total and an immutable
/// `created_at`, so every rescan grew the creation day and gave the days
/// the tokens were actually spent on nothing. `session.shutdown` records
/// carry their own timestamp, so usage lands on the day it happened.

#[test]
fn parse_iso8601_handles_space_separated_fractional_seconds() {
    // SQLite datetime() text form; must not fall through to the 1970 default.
    let ms = parse_iso8601_timestamp_ms("2026-07-01 12:34:56.789")
        .expect("space + fractional seconds should parse");
    assert_eq!(ms, 1_782_909_296_789);

    // Sibling formats still parse.
    assert_eq!(
        parse_iso8601_timestamp_ms("2026-07-01T12:34:56Z"),
        Some(1_782_909_296_000)
    );
    assert_eq!(
        parse_iso8601_timestamp_ms("2026-07-01 12:34:56"),
        Some(1_782_909_296_000)
    );
    assert_eq!(parse_iso8601_timestamp_ms("not-a-timestamp"), None);
}
