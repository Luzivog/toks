use super::*;

#[test]
fn test_token_count_uses_total_deltas_when_totals_repeat() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);
}

#[test]
fn test_token_count_falls_back_to_last_usage_when_totals_reset() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);
    assert_eq!(messages[1].tokens.input, 8);
    assert_eq!(messages[1].tokens.output, 2);
    assert_eq!(messages[1].tokens.cache_read, 2);
    assert_eq!(messages[1].tokens.reasoning, 1);
}

#[test]
fn test_token_count_advances_baseline_after_missing_total_fallback() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":110,"cached_input_tokens":22,"output_tokens":33,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}\n{}", line1, line2, line3, line4);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);
    assert_eq!(messages[1].tokens.input, 8);
    assert_eq!(messages[1].tokens.output, 2);
    assert_eq!(messages[1].tokens.cache_read, 2);
    assert_eq!(messages[1].tokens.reasoning, 1);
}

#[test]
fn test_token_count_skips_regressed_totals_without_last_usage() {
    // When totals regress and last_usage is absent, the row should be
    // skipped entirely to avoid double-counting the full cumulative total.
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    // Totals regress (lower values) and no last_token_usage — should skip
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":50,"cached_input_tokens":10,"output_tokens":15,"reasoning_output_tokens":2}}}}"#;
    // Normal continuation after reset
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":80,"cached_input_tokens":15,"output_tokens":25,"reasoning_output_tokens":4}}}}"#;
    let content = format!("{}\n{}\n{}\n{}", line1, line2, line3, line4);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    // Should produce 2 messages: first from line2 (full total),
    // then delta from line4 relative to line3 (baseline reset).
    assert_eq!(messages.len(), 2);
    // First message: full total
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);
    // Second message: delta from 50→80
    assert_eq!(messages[1].tokens.input, 25);
    assert_eq!(messages[1].tokens.output, 8);
    assert_eq!(messages[1].tokens.cache_read, 5);
    assert_eq!(messages[1].tokens.reasoning, 2);
}

#[test]
fn test_into_tokens_clamps_cached_to_input() {
    // When cached > input (malformed data), cached should be clamped to input
    // so that input + cache_read never exceeds the raw input value.
    let totals = CodexTotals {
        input: 50,
        output: 30,
        cached: 100, // More than input — malformed
        reasoning: 5,
    };
    let tokens = totals.into_tokens();
    assert_eq!(tokens.cache_read, 50); // Clamped to input
    assert_eq!(tokens.input, 0); // input - clamped_cached = 0
    assert_eq!(tokens.output, 25);
    assert_eq!(tokens.reasoning, 5);
}

#[test]
fn test_token_count_ignores_negative_fallback_usage_in_baseline() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":-10,"cached_input_tokens":-2,"output_tokens":-3,"reasoning_output_tokens":-1}}}}"#;
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":110,"cached_input_tokens":22,"output_tokens":33,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}\n{}", line1, line2, line3, line4);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);
    assert_eq!(messages[1].tokens.input, 8);
    assert_eq!(messages[1].tokens.output, 2);
    assert_eq!(messages[1].tokens.cache_read, 2);
    assert_eq!(messages[1].tokens.reasoning, 1);
}

#[test]
fn test_token_count_avoids_double_counting_stale_cumulative_regressions() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":110,"cached_input_tokens":22,"output_tokens":33,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":109,"cached_input_tokens":21,"output_tokens":32,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":9,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":0}}}}"#;
    let line5 = r#"{"timestamp":"2026-01-01T00:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":119,"cached_input_tokens":23,"output_tokens":35,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":0}}}}"#;
    let content = format!("{}\n{}\n{}\n{}\n{}", line1, line2, line3, line4, line5);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[0].tokens.output, 25);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.reasoning, 5);

    assert_eq!(messages[1].tokens.input, 8);
    assert_eq!(messages[1].tokens.output, 2);
    assert_eq!(messages[1].tokens.cache_read, 2);
    assert_eq!(messages[1].tokens.reasoning, 1);

    // Stale snapshot (line4) is now skipped entirely; messages[2]
    // comes from line5's last_token_usage instead.
    assert_eq!(messages[2].tokens.input, 8);
    assert_eq!(messages[2].tokens.output, 3);
    assert_eq!(messages[2].tokens.cache_read, 2);
    assert_eq!(messages[2].tokens.reasoning, 0);
}

#[test]
fn test_token_count_handles_multiple_stale_regressions_before_recovery() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5},"last_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":30,"reasoning_output_tokens":5}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":110,"cached_input_tokens":22,"output_tokens":33,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":109,"cached_input_tokens":21,"output_tokens":32,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":9,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":0}}}}"#;
    let line5 = r#"{"timestamp":"2026-01-01T00:00:04Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":118,"cached_input_tokens":22,"output_tokens":34,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":9,"cached_input_tokens":1,"output_tokens":2,"reasoning_output_tokens":0}}}}"#;
    let line6 = r#"{"timestamp":"2026-01-01T00:00:05Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":128,"cached_input_tokens":24,"output_tokens":37,"reasoning_output_tokens":6},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":0}}}}"#;
    let content = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        line1, line2, line3, line4, line5, line6
    );
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    // Stale line4 is skipped; messages come from lines 2, 3, 5, 6.
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].tokens.input, 80);
    assert_eq!(messages[1].tokens.input, 8);
    assert_eq!(messages[2].tokens.input, 8);
    assert_eq!(messages[2].tokens.output, 2);
    assert_eq!(messages[2].tokens.cache_read, 1);
    assert_eq!(messages[2].tokens.reasoning, 0);
    assert_eq!(messages[3].tokens.input, 8);
    assert_eq!(messages[3].tokens.output, 3);
    assert_eq!(messages[3].tokens.cache_read, 2);
    assert_eq!(messages[3].tokens.reasoning, 0);
}

#[test]
fn test_token_count_treats_large_regressions_as_real_resets() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10000,"cached_input_tokens":1000,"output_tokens":400,"reasoning_output_tokens":50},"last_token_usage":{"input_tokens":10000,"cached_input_tokens":1000,"output_tokens":400,"reasoning_output_tokens":50}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":7600,"cached_input_tokens":800,"output_tokens":280,"reasoning_output_tokens":35},"last_token_usage":{"input_tokens":25,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1}}}}"#;
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":7625,"cached_input_tokens":805,"output_tokens":284,"reasoning_output_tokens":36},"last_token_usage":{"input_tokens":25,"cached_input_tokens":5,"output_tokens":4,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}\n{}", line1, line2, line3, line4);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].tokens.input, 9000);
    assert_eq!(messages[0].tokens.output, 350);
    assert_eq!(messages[0].tokens.cache_read, 1000);
    assert_eq!(messages[0].tokens.reasoning, 50);

    assert_eq!(messages[1].tokens.input, 20);
    assert_eq!(messages[1].tokens.output, 3);
    assert_eq!(messages[1].tokens.cache_read, 5);
    assert_eq!(messages[1].tokens.reasoning, 1);

    assert_eq!(messages[2].tokens.input, 20);
    assert_eq!(messages[2].tokens.output, 3);
    assert_eq!(messages[2].tokens.cache_read, 5);
    assert_eq!(messages[2].tokens.reasoning, 1);
}

#[test]
fn test_first_event_uses_last_not_total_for_resumed_sessions() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":5000,"cached_input_tokens":500,"output_tokens":800,"reasoning_output_tokens":100},"last_token_usage":{"input_tokens":12,"cached_input_tokens":2,"output_tokens":5,"reasoning_output_tokens":1}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":5012,"cached_input_tokens":502,"output_tokens":805,"reasoning_output_tokens":101},"last_token_usage":{"input_tokens":12,"cached_input_tokens":2,"output_tokens":5,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].tokens.output, 4);
    assert_eq!(messages[0].tokens.cache_read, 2);
    assert_eq!(messages[0].tokens.reasoning, 1);
    assert_eq!(messages[1].tokens.input, 10);
    assert_eq!(messages[1].tokens.output, 4);
    assert_eq!(messages[1].tokens.cache_read, 2);
    assert_eq!(messages[1].tokens.reasoning, 1);
}

#[test]
fn test_zero_token_snapshot_does_not_inflate_later_deltas() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":500,"cached_input_tokens":50,"output_tokens":80,"reasoning_output_tokens":10},"last_token_usage":{"input_tokens":500,"cached_input_tokens":50,"output_tokens":80,"reasoning_output_tokens":10}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0},"last_token_usage":{"input_tokens":0,"cached_input_tokens":0,"output_tokens":0,"reasoning_output_tokens":0}}}}"#;
    let line4 = r#"{"timestamp":"2026-01-01T00:00:03Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":510,"cached_input_tokens":52,"output_tokens":83,"reasoning_output_tokens":11},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3,"reasoning_output_tokens":1}}}}"#;
    let content = format!("{}\n{}\n{}\n{}", line1, line2, line3, line4);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].tokens.input, 450);
    assert_eq!(messages[0].tokens.output, 70);
    assert_eq!(messages[0].tokens.cache_read, 50);
    assert_eq!(messages[0].tokens.reasoning, 10);
    assert_eq!(messages[1].tokens.input, 8);
    assert_eq!(messages[1].tokens.output, 2);
    assert_eq!(messages[1].tokens.cache_read, 2);
    assert_eq!(messages[1].tokens.reasoning, 1);
}

#[test]
fn test_cached_tokens_takes_max_of_both_fields() {
    let usage = CodexTokenUsage {
        input_tokens: Some(100),
        output_tokens: Some(30),
        cached_input_tokens: Some(10),
        cache_read_input_tokens: Some(20),
        reasoning_output_tokens: Some(5),
        total_tokens: None,
    };
    let totals = CodexTotals::from_usage(&usage);
    assert_eq!(totals.cached, 20);
}

#[test]
fn test_compaction_total_drop_uses_last_as_increment() {
    let line1 = r#"{"timestamp":"2026-01-01T00:00:00Z","type":"turn_context","payload":{"model":"gpt-5.2"}}"#;
    let line2 = r#"{"timestamp":"2026-01-01T00:00:01Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":150000,"cached_input_tokens":10000,"output_tokens":20000,"reasoning_output_tokens":5000},"last_token_usage":{"input_tokens":150000,"cached_input_tokens":10000,"output_tokens":20000,"reasoning_output_tokens":5000}}}}"#;
    let line3 = r#"{"timestamp":"2026-01-01T00:00:02Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":200000,"cached_input_tokens":15000,"output_tokens":25000,"reasoning_output_tokens":6000},"last_token_usage":{"input_tokens":50,"cached_input_tokens":5,"output_tokens":10,"reasoning_output_tokens":2}}}}"#;
    let content = format!("{}\n{}\n{}", line1, line2, line3);
    let file = create_test_file(&content);

    let messages = parse_codex_file(file.path());

    assert_eq!(messages.len(), 2);
    assert_eq!(messages[1].tokens.input, 45);
    assert_eq!(messages[1].tokens.output, 8);
    assert_eq!(messages[1].tokens.cache_read, 5);
    assert_eq!(messages[1].tokens.reasoning, 2);
}
