use super::*;
use std::io::Write;
use tempfile::TempDir;

fn parse_events(content: &str) -> Vec<UnifiedMessage> {
    let dir = TempDir::new().unwrap();
    let repo_dir = dir.path().join("test-repo");
    std::fs::create_dir_all(&repo_dir).unwrap();
    let path = repo_dir.join("test-session-123.jsonl");
    let mut file = std::fs::File::create(&path).unwrap();
    file.write_all(content.as_bytes()).unwrap();
    file.flush().unwrap();
    parse_opencodereview_file(&path)
}

fn session_start(cwd: &str) -> String {
    format!(
        r#"{{"type":"session_start","sessionId":"test-session-123","timestamp":"2026-01-15T10:00:00Z","cwd":"{cwd}","model":"claude-sonnet-4-20250514"}}"#
    )
}

fn llm_response(
    timestamp: &str,
    model: &str,
    prompt: i64,
    completion: i64,
    cache_read: i64,
    cache_write: i64,
) -> String {
    format!(
        r#"{{"type":"llm_response","sessionId":"test-session-123","timestamp":"{timestamp}","model":"{model}","duration_ms":1500,"usage":{{"prompt_tokens":{prompt},"completion_tokens":{completion},"cache_read_tokens":{cache_read},"cache_write_tokens":{cache_write}}}}}"#
    )
}

fn llm_response_without_timestamp(
    model: &str,
    duration_ms: i64,
    prompt: i64,
    completion: i64,
) -> String {
    format!(
        r#"{{"type":"llm_response","sessionId":"test-session-123","model":"{model}","duration_ms":{duration_ms},"usage":{{"prompt_tokens":{prompt},"completion_tokens":{completion},"cache_read_tokens":0,"cache_write_tokens":0}}}}"#
    )
}

#[test]
fn parses_single_llm_response() {
    let content = format!(
        "{}\n{}\n",
        session_start("/home/user/project"),
        llm_response(
            "2026-01-15T10:00:05Z",
            "claude-sonnet-4-20250514",
            1000,
            200,
            500,
            100
        ),
    );
    let msgs = parse_events(&content);

    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].client, "opencodereview");
    assert_eq!(msgs[0].tokens.input, 1000);
    assert_eq!(msgs[0].tokens.output, 200);
    assert_eq!(msgs[0].tokens.cache_read, 500);
    assert_eq!(msgs[0].tokens.cache_write, 100);
    assert_eq!(msgs[0].tokens.reasoning, 0);
    assert_eq!(msgs[0].duration_ms, Some(1500));
    assert_eq!(msgs[0].session_id, "test-session-123");
    assert!(msgs[0].workspace_key.is_some());
}

#[test]
fn clamps_extreme_unsigned_usage_and_keeps_the_message() {
    let content = format!(
        "{}\n{}\n",
        session_start("/home/user/project"),
        r#"{"type":"llm_response","sessionId":"test-session-123","timestamp":"2026-01-15T10:00:05Z","model":"gpt-4o","duration_ms":1500,"usage":{"prompt_tokens":18446744073709551615,"completion_tokens":9223372036854775807,"cache_read_tokens":-1,"cache_write_tokens":0}}"#,
    );
    let msgs = parse_events(&content);

    assert_eq!(
        msgs.len(),
        1,
        "one extreme bucket must not drop the message"
    );
    assert_eq!(msgs[0].tokens.input, i64::MAX);
    assert_eq!(msgs[0].tokens.output, i64::MAX);
    assert_eq!(msgs[0].tokens.cache_read, 0);
    assert_eq!(msgs[0].tokens.cache_write, 0);
}

#[test]
fn parses_multiple_responses() {
    let content = format!(
        "{}\n{}\n{}\n",
        session_start("/home/user/project"),
        llm_response(
            "2026-01-15T10:00:05Z",
            "claude-sonnet-4-20250514",
            1000,
            200,
            0,
            0
        ),
        llm_response("2026-01-15T10:01:00Z", "gpt-4o", 500, 100, 0, 0),
    );
    let msgs = parse_events(&content);
    assert_eq!(msgs.len(), 2);
}

#[test]
fn deduplicates_identical_records() {
    let resp = llm_response(
        "2026-01-15T10:00:05Z",
        "claude-sonnet-4-20250514",
        1000,
        200,
        0,
        0,
    );
    let content = format!(
        "{}\n{}\n{}\n",
        session_start("/home/user/project"),
        resp,
        resp,
    );
    let msgs = parse_events(&content);
    assert_eq!(msgs.len(), 1, "duplicate records should be collapsed");
}

#[test]
fn timestampless_records_with_identical_usage_stay_distinct() {
    // Regression (#941 finding 2): without a `timestamp` field every
    // record in the file falls back to the same file mtime, so the
    // session/timestamp/model/usage key could not tell two genuinely
    // distinct calls apart and silently collapsed them into one.
    let content = format!(
        "{}\n{}\n{}\n",
        session_start("/home/user/project"),
        llm_response_without_timestamp("gpt-4o", 1200, 100, 50),
        llm_response_without_timestamp("gpt-4o", 3400, 100, 50),
    );
    let msgs = parse_events(&content);

    assert_eq!(
        msgs.len(),
        2,
        "two distinct timestampless calls must not collapse into one"
    );
    assert_ne!(
        msgs[0].dedup_key, msgs[1].dedup_key,
        "distinct timestampless records need distinct dedup keys"
    );
    assert_eq!(msgs[0].duration_ms, Some(1200));
    assert_eq!(msgs[1].duration_ms, Some(3400));
}

#[test]
fn reparsing_a_timestampless_file_reproduces_the_same_dedup_keys() {
    // The discriminator must be a pure function of the file's bytes, not
    // of parse order or wall-clock, so two parses of an unchanged file
    // agree. Only this parser's own `seen` set reads the key today —
    // unified_to_parsed drops the field — so this pins the property
    // before a cross-parse consumer relies on it. Note the rest of the
    // key still moves when the file grows: with no `timestamp` field
    // `recorded_timestamp` is the mtime.
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("test-session-123.jsonl");
    std::fs::write(
        &path,
        format!(
            "{}\n{}\n{}\n",
            session_start("/home/user/project"),
            llm_response_without_timestamp("gpt-4o", 1200, 100, 50),
            llm_response_without_timestamp("gpt-4o", 3400, 100, 50),
        ),
    )
    .unwrap();

    let first: Vec<_> = parse_opencodereview_file(&path)
        .into_iter()
        .map(|msg| msg.dedup_key)
        .collect();
    let second: Vec<_> = parse_opencodereview_file(&path)
        .into_iter()
        .map(|msg| msg.dedup_key)
        .collect();

    assert_eq!(first.len(), 2);
    assert_eq!(
        first, second,
        "re-parsing an unchanged file must reproduce the same dedup keys"
    );
}

#[test]
fn skips_zero_token_records() {
    let content = format!(
        "{}\n{}\n",
        session_start("/home/user/project"),
        llm_response(
            "2026-01-15T10:00:05Z",
            "claude-sonnet-4-20250514",
            0,
            0,
            0,
            0
        ),
    );
    let msgs = parse_events(&content);
    assert_eq!(msgs.len(), 0, "zero-token records should be skipped");
}

#[test]
fn works_without_session_start() {
    let content = format!(
        "{}\n",
        llm_response(
            "2026-01-15T10:00:05Z",
            "claude-sonnet-4-20250514",
            1000,
            200,
            0,
            0
        ),
    );
    let msgs = parse_events(&content);
    assert_eq!(msgs.len(), 1);
    assert!(msgs[0].workspace_key.is_none());
}

#[test]
fn session_id_derived_from_filename() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("my-unique-session.jsonl");
    let content = llm_response("2026-01-15T10:00:05Z", "gpt-4o", 100, 50, 0, 0);
    std::fs::write(&path, format!("{content}\n")).unwrap();

    let msgs = parse_opencodereview_file(&path);
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].session_id, "my-unique-session");
}

#[test]
fn test_llm_response_timestamp_is_start_anchored() {
    // Regression (follow-up to #890): an `llm_response` record's
    // `timestamp` is written when the response is logged, i.e. the
    // call's *end*, not its start. `duration_ms` is that call's elapsed
    // time, so sessionize()'s `[timestamp, timestamp + duration_ms]`
    // span would otherwise project forward past the actual completion
    // into phantom idle time. The parser must back-calculate the start
    // anchor instead.
    let content = llm_response("2026-01-15T10:00:05Z", "gpt-4o", 100, 50, 0, 0);
    let msgs = parse_events(&format!("{content}\n"));

    assert_eq!(msgs.len(), 1);
    let expected_end =
        parse_timestamp_value(&Value::String("2026-01-15T10:00:05Z".to_string())).unwrap();
    assert_eq!(
        msgs[0].timestamp,
        expected_end - 1500,
        "timestamp must be back-calculated to the call start (end - duration)"
    );
    assert_eq!(
        msgs[0].duration_ms,
        Some(1500),
        "duration_ms must still span from start to the recorded end timestamp"
    );
}
