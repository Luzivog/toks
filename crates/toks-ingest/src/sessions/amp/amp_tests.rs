use super::parse_amp_file;
use std::path::Path;

fn write_amp_thread(path: &Path, content: &str) {
    std::fs::write(path, content).unwrap();
}

fn timestamp_ms(value: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(value)
        .unwrap()
        .timestamp_millis()
}

fn local_date(timestamp_ms: i64) -> String {
    use chrono::TimeZone;

    chrono::Local
        .timestamp_millis_opt(timestamp_ms)
        .single()
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

#[test]
fn test_parse_amp_reconciles_partial_ledger_with_message_usage() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("T-partial.json");
    let thread_created = timestamp_ms("2026-04-04T12:00:00Z");
    let ledger_timestamp = "2026-04-08T12:00:00Z";

    write_amp_thread(
        &path,
        &serde_json::json!({
            "id": "thread-partial",
            "created": thread_created,
            "usageLedger": {
                "events": [
                    {
                        "timestamp": ledger_timestamp,
                        "model": "claude-sonnet-4-0",
                        "credits": 0.75,
                        "tokens": { "input": 100, "output": 20 }
                    }
                ]
            },
            "messages": [
                {
                    "role": "assistant",
                    "messageId": 1,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 100,
                        "outputTokens": 20,
                        "credits": 0.75
                    }
                },
                {
                    "role": "assistant",
                    "messageId": 2,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 50,
                        "outputTokens": 10,
                        "credits": 0.40
                    }
                }
            ]
        })
        .to_string(),
    );

    let messages = parse_amp_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].date, local_date(thread_created + 2000));
    assert_eq!(messages[1].date, local_date(timestamp_ms(ledger_timestamp)));
    assert_eq!(messages[0].tokens.input, 50);
    assert_eq!(messages[1].tokens.input, 100);
}

#[test]
fn test_parse_amp_does_not_double_count_full_ledger() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("T-full.json");
    let thread_created = timestamp_ms("2026-04-04T12:00:00Z");
    let first_ledger_timestamp = "2026-04-04T12:00:00Z";
    let second_ledger_timestamp = "2026-04-05T12:00:00Z";

    write_amp_thread(
        &path,
        &serde_json::json!({
            "id": "thread-full",
            "created": thread_created,
            "usageLedger": {
                "events": [
                    {
                        "timestamp": first_ledger_timestamp,
                        "model": "claude-sonnet-4-0",
                        "credits": 0.20,
                        "tokens": { "input": 20, "output": 5 }
                    },
                    {
                        "timestamp": second_ledger_timestamp,
                        "model": "claude-sonnet-4-0",
                        "credits": 0.25,
                        "tokens": { "input": 25, "output": 5 }
                    }
                ]
            },
            "messages": [
                {
                    "role": "assistant",
                    "messageId": 1,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 20,
                        "outputTokens": 5,
                        "credits": 0.20
                    }
                },
                {
                    "role": "assistant",
                    "messageId": 2,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 25,
                        "outputTokens": 5,
                        "credits": 0.25
                    }
                }
            ]
        })
        .to_string(),
    );

    let messages = parse_amp_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages[0].date,
        local_date(timestamp_ms(first_ledger_timestamp))
    );
    assert_eq!(
        messages[1].date,
        local_date(timestamp_ms(second_ledger_timestamp))
    );
}

#[test]
fn test_parse_amp_prefers_message_id_match_over_token_heuristic() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("T-message-id-match.json");
    let thread_created = timestamp_ms("2026-04-04T12:00:00Z");
    let first_ledger_timestamp = "2026-04-10T12:00:00Z";
    let second_ledger_timestamp = "2026-04-05T12:00:00Z";

    write_amp_thread(
        &path,
        &serde_json::json!({
            "id": "thread-message-id-match",
            "created": thread_created,
            "usageLedger": {
                "events": [
                    {
                        "timestamp": first_ledger_timestamp,
                        "model": "claude-sonnet-4-0",
                        "credits": 0.20,
                        "tokens": { "input": 20, "output": 5 },
                        "toMessageId": 2
                    },
                    {
                        "timestamp": second_ledger_timestamp,
                        "model": "claude-sonnet-4-0",
                        "credits": 0.20,
                        "tokens": { "input": 20, "output": 5 },
                        "toMessageId": 1
                    }
                ]
            },
            "messages": [
                {
                    "role": "assistant",
                    "messageId": 1,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 20,
                        "outputTokens": 5,
                        "credits": 0.20
                    }
                },
                {
                    "role": "assistant",
                    "messageId": 2,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 20,
                        "outputTokens": 5,
                        "credits": 0.20
                    }
                }
            ]
        })
        .to_string(),
    );

    let messages = parse_amp_file(&path);
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].timestamp, timestamp_ms(second_ledger_timestamp));
    assert_eq!(messages[1].timestamp, timestamp_ms(first_ledger_timestamp));
}

#[test]
fn test_parse_amp_prefers_message_timestamp_when_ledger_timestamp_missing() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("T-missing-ledger-ts.json");
    let thread_created = timestamp_ms("2026-04-04T12:00:00Z");

    write_amp_thread(
        &path,
        &serde_json::json!({
            "id": "thread-missing-ts",
            "created": thread_created,
            "usageLedger": {
                "events": [
                    {
                        "model": "claude-sonnet-4-0",
                        "credits": 0.20,
                        "tokens": { "input": 20, "output": 5 }
                    }
                ]
            },
            "messages": [
                {
                    "role": "assistant",
                    "messageId": 7,
                    "usage": {
                        "model": "claude-sonnet-4-0",
                        "inputTokens": 20,
                        "outputTokens": 5,
                        "credits": 0.20
                    }
                }
            ]
        })
        .to_string(),
    );

    let messages = parse_amp_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].timestamp, thread_created + 7000);
}

#[test]
fn test_parse_amp_uses_file_mtime_when_thread_created_missing() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    let path = temp_dir.path().join("T-no-created.json");

    write_amp_thread(
        &path,
        r#"{
                "id": "thread-no-created",
                "messages": [
                    {
                        "role": "assistant",
                        "messageId": 5,
                        "usage": {
                            "model": "claude-sonnet-4-0",
                            "inputTokens": 10,
                            "outputTokens": 2,
                            "credits": 0.11
                        }
                    }
                ]
            }"#,
    );

    let file_mtime_ms = crate::sessions::utils::file_modified_timestamp_ms(&path);
    let messages = parse_amp_file(&path);
    assert_eq!(messages.len(), 1);
    assert!(messages[0].timestamp >= file_mtime_ms);
    assert_ne!(messages[0].date, "1970-01-01");
}
