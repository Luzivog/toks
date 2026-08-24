use super::*;

const CODEX_DURATION_FIXTURE: &str = concat!(
    r#"{"timestamp":"2040-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
    "\n",
    r#"{"timestamp":"2040-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":50,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0},"last_token_usage":{"input_tokens":50,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}"#,
    "\n",
    r#"{"timestamp":"2040-01-01T00:00:02Z","type":"event_msg","payload":{"type":"agent_message"}}"#,
    "\n",
    r#"{"timestamp":"2040-01-01T00:00:03Z","type":"event_msg","payload":{"type":"agent_message"}}"#,
    "\n",
    r#"{"timestamp":"2040-01-01T00:00:04Z","type":"event_msg","payload":{"type":"agent_message"}}"#,
    "\n",
    r#"{"timestamp":"2040-01-01T00:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":110,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0},"last_token_usage":{"input_tokens":60,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}"#,
    "\n",
    r#"{"timestamp":"2040-01-01T00:00:07Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":170,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0},"last_token_usage":{"input_tokens":60,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}"#,
    "\n"
);

#[test]
fn test_token_count_durations_are_non_overlapping() {
    let file = create_test_file(CODEX_DURATION_FIXTURE);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 3);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.duration_ms)
            .collect::<Vec<_>>(),
        vec![Some(1_000), Some(4_000), Some(2_000)]
    );
}

#[test]
fn test_token_count_durations_ignore_invalid_equal_and_backward_timestamps() {
    let file = create_test_file(concat!(
        r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#,
        "\n",
        r#"{"timestamp":"not-a-timestamp","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15,"cached_input_tokens":3,"output_tokens":5,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":0}}}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":4,"output_tokens":6,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":1,"reasoning_output_tokens":0}}}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:00.500Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":25,"cached_input_tokens":5,"output_tokens":8,"reasoning_output_tokens":2},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":1}}}}"#,
        "\n",
        r#"{"timestamp":"2026-01-01T00:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":35,"cached_input_tokens":7,"output_tokens":11,"reasoning_output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#,
        "\n"
    ));

    let parsed = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());

    assert!(parsed.parse_succeeded);
    assert_eq!(parsed.messages.len(), 5);
    assert_eq!(
        parsed
            .messages
            .iter()
            .map(|message| message.duration_ms)
            .collect::<Vec<_>>(),
        vec![Some(1_000), None, None, None, Some(3_000)]
    );
    assert_eq!(
        parsed.state.last_accepted_token_timestamp_ms,
        parse_codex_entry_timestamp(Some("2026-01-01T00:00:04Z"))
    );
    assert_eq!(
        parsed.consumed_offset,
        file.as_file().metadata().unwrap().len()
    );
}

#[test]
fn test_duration_fixture_incremental_parse_matches_full_parse() {
    let lines = CODEX_DURATION_FIXTURE.lines().collect::<Vec<_>>();
    let initial_content = format!("{}\n", lines[..5].join("\n"));
    let appended_content = format!("{}\n", lines[5..].join("\n"));
    let file = create_test_file(&initial_content);
    let initial_size = file.as_file().metadata().unwrap().len();

    let initial = parse_codex_file_incremental(file.path(), 0, CodexParseState::default());
    assert_eq!(initial.messages.len(), 1);
    assert_eq!(initial.messages[0].duration_ms, Some(1_000));
    assert_eq!(
        initial.state.last_accepted_token_timestamp_ms,
        parse_codex_entry_timestamp(Some("2040-01-01T00:00:01Z"))
    );

    let mut reopened = file.reopen().unwrap();
    reopened.seek(SeekFrom::End(0)).unwrap();
    reopened.write_all(appended_content.as_bytes()).unwrap();
    reopened.flush().unwrap();

    let incremental =
        parse_codex_file_incremental(file.path(), initial_size, initial.state.clone());
    let mut combined = initial.messages;
    combined.extend(incremental.messages);

    let full = parse_codex_file(file.path());
    assert_eq!(combined, full);
    assert_eq!(
        full.iter()
            .map(|message| message.duration_ms)
            .collect::<Vec<_>>(),
        vec![Some(1_000), Some(4_000), Some(2_000)]
    );
}

#[test]
fn test_duration_fixture_aggregates_and_serializes_performance() {
    let file = create_test_file(CODEX_DURATION_FIXTURE);
    let messages = parse_codex_file(file.path());

    let entries = aggregate_model_usage_entries(messages, &GroupBy::ClientModel);

    assert_eq!(entries.len(), 1);
    let performance = &entries[0].performance;
    assert_eq!(performance.total_duration_ms, 7_000);
    assert_eq!(performance.timed_tokens, 170);
    assert_eq!(performance.sample_count, 3);
    assert_eq!(performance.token_coverage, 1.0);
    let expected_ms_per_1k = 7_000.0 * 1_000.0 / 170.0;
    assert!((performance.ms_per_1k_tokens.unwrap() - expected_ms_per_1k).abs() < f64::EPSILON);

    let json = serde_json::to_value(performance).unwrap();
    assert_eq!(json["totalDurationMs"], 7_000);
    assert_eq!(json["timedTokens"], 170);
    assert_eq!(json["sampleCount"], 3);
    assert_eq!(json["tokenCoverage"], 1.0);
    assert!(json["msPer1KTokens"].is_number());
    assert!(json.get("total_duration_ms").is_none());
}

#[test]
fn test_token_count_timestamp_is_start_anchored() {
    let line1 = r#"{"timestamp":"1970-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"1970-01-01T00:00:01.005Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#;
    let content = format!("{}\n{}", line1, line2);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].timestamp, 1_000,
        "timestamp must be the turn_context start (1000ms epoch)"
    );
    assert_eq!(
        messages[0].duration_ms,
        Some(5),
        "duration_ms must span from turn start to token_count event (5ms)"
    );
}

#[test]
fn test_user_message_without_turn_context_anchors_at_user_message() {
    // Regression: a resumed/compacted session can emit a human
    // `user_message` followed directly by a `token_count` with no
    // intervening `turn_context` (which normally resets the start-anchor
    // cursor every turn). Before this fix, the token_count would anchor
    // at the previous turn's last accepted token timestamp instead of
    // this user message, bridging backward across the idle gap between
    // turns.
    let line1 = r#"{"timestamp":"1970-01-01T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"1970-01-01T00:00:01.100Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#;
    // A long idle gap follows: the session resumes with a human
    // user_message but no fresh turn_context before the next token_count.
    let line3 = r#"{"timestamp":"1970-01-01T01:00:00Z","type":"event_msg","payload":{"type":"user_message","message":"still there?"}}"#;
    let line4 = r#"{"timestamp":"1970-01-01T01:00:00.500Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":20,"cached_input_tokens":4,"output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#;
    let content = [line1, line2, line3, line4].join("\n");
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[1].timestamp,
        parse_codex_entry_timestamp(Some("1970-01-01T01:00:00Z")).unwrap(),
        "the second token_count must anchor at the user_message, not the \
             previous turn's last accepted token timestamp"
    );
    assert_eq!(
        messages[1].duration_ms,
        Some(500),
        "duration_ms must span from the user_message to its token_count \
             (500ms), not bridge backward across the idle gap"
    );
    assert!(
        messages[1].is_turn_start,
        "the deferred turn-start marker must still apply"
    );
}
