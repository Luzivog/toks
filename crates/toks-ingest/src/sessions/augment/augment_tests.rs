use super::*;
use std::io::Write;
use std::time::{SystemTime, UNIX_EPOCH};
use tempfile::NamedTempFile;

fn write_temp_json(json: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(json.as_bytes()).unwrap();
    f
}

#[test]
fn test_parse_valid_session_one_message_per_turn() {
    let json = r#"{
            "sessionId": "11111111-2222-3333-4444-555555555555",
            "created": "2026-01-15T12:00:00.000Z",
            "modified": "2026-01-15T12:10:00.000Z",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                {
                    "completed": true,
                    "finishedAt": "2026-01-15T12:01:00.000Z",
                    "sequenceId": 1,
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "req-1",
                        "response_nodes": [
                            { "type": 1 },
                            {
                                "type": 10,
                                "token_usage": {
                                    "input_tokens": 1000,
                                    "output_tokens": 50,
                                    "cache_read_input_tokens": 200,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                },
                {
                    "completed": true,
                    "finishedAt": "2026-01-15T12:05:00.000Z",
                    "sequenceId": 2,
                    "exchange": {
                        "model_id": "claude-opus-4-8",
                        "request_id": "req-2",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 400,
                                    "output_tokens": 100,
                                    "cache_read_input_tokens": 800,
                                    "cache_creation_input_tokens": 25
                                }
                            }
                        ]
                    }
                },
                {
                    "completed": false,
                    "finishedAt": "2026-01-15T12:09:00.000Z",
                    "exchange": {
                        "model_id": "grok-4-5",
                        "response_nodes": []
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 2);

    let first = &messages[0];
    assert_eq!(first.client, "augment");
    assert_eq!(first.session_id, "11111111-2222-3333-4444-555555555555");
    assert_eq!(first.model_id, "grok-4-5");
    assert_eq!(first.provider_id, "xai");
    assert_eq!(first.tokens.input, 1000);
    assert_eq!(first.tokens.output, 50);
    assert_eq!(first.tokens.cache_read, 200);
    assert_eq!(first.tokens.cache_write, 0);
    assert!(first.is_turn_start);
    assert_eq!(
        first.dedup_key.as_deref(),
        Some("augment:11111111-2222-3333-4444-555555555555:req:req-1")
    );
    assert_eq!(
        first.timestamp,
        parse_timestamp_str("2026-01-15T12:01:00.000Z").unwrap()
    );

    let second = &messages[1];
    assert_eq!(second.model_id, "claude-opus-4-8");
    assert_eq!(second.provider_id, "anthropic");
    assert_eq!(second.tokens.input, 400);
    assert_eq!(second.tokens.output, 100);
    assert_eq!(second.tokens.cache_read, 800);
    assert_eq!(second.tokens.cache_write, 25);
}

#[test]
fn test_falls_back_to_session_model_and_filename_session_id() {
    let json = r#"{
            "agentState": { "modelId": "gpt-5-4" },
            "chatHistory": [
                {
                    "completed": true,
                    "finishedAt": "2026-07-20T13:33:00.000Z",
                    "sequenceId": "seq-a",
                    "exchange": {
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 10,
                                    "output_tokens": 5,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("session-from-name.json");
    std::fs::write(&path, json).unwrap();
    let messages = parse_augment_file(&path);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].session_id, "session-from-name");
    assert_eq!(messages[0].model_id, "gpt-5-4");
    assert_eq!(messages[0].provider_id, "openai");
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("augment:session-from-name:seq:seq-a")
    );
}

#[test]
fn test_prefers_last_nonempty_usage_node_when_totals_diverge() {
    // If a future format streams a partial usage then a fuller final one,
    // take the last non-empty observation (not the first, not the sum).
    let json = r#"{
            "sessionId": "s1",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                {
                    "completed": true,
                    "finishedAt": "2026-07-20T13:33:00.000Z",
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "r1",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 100,
                                    "output_tokens": 10,
                                    "cache_read_input_tokens": 50,
                                    "cache_creation_input_tokens": 0
                                }
                            },
                            {
                                "token_usage": {
                                    "input_tokens": 250,
                                    "output_tokens": 40,
                                    "cache_read_input_tokens": 75,
                                    "cache_creation_input_tokens": 5
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 250);
    assert_eq!(messages[0].tokens.output, 40);
    assert_eq!(messages[0].tokens.cache_read, 75);
    assert_eq!(messages[0].tokens.cache_write, 5);
}

#[test]
fn test_identical_multi_usage_nodes_still_count_once() {
    let json = r#"{
            "sessionId": "s1b",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                {
                    "completed": true,
                    "finishedAt": "2026-07-20T13:33:00.000Z",
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "r1",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 100,
                                    "output_tokens": 10,
                                    "cache_read_input_tokens": 50,
                                    "cache_creation_input_tokens": 0
                                }
                            },
                            {
                                "token_usage": {
                                    "input_tokens": 100,
                                    "output_tokens": 10,
                                    "cache_read_input_tokens": 50,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 10);
    assert_eq!(messages[0].tokens.cache_read, 50);
}

#[test]
fn test_invalid_json_and_missing_file() {
    let f = write_temp_json("not json");
    assert!(parse_augment_file(f.path()).is_empty());
    assert!(parse_augment_file(Path::new("/nonexistent/augment.json")).is_empty());
}

#[test]
fn test_skips_malformed_turns() {
    let json = r#"{
            "sessionId": "s2",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                "bad-turn",
                {
                    "completed": true,
                    "finishedAt": "2026-07-20T13:33:00.000Z",
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "ok",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 1,
                                    "output_tokens": 2,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 1);
    assert_eq!(messages[0].tokens.output, 2);
}

#[test]
fn test_skips_incomplete_turns_even_with_token_usage() {
    let json = r#"{
            "sessionId": "s3",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                {
                    "completed": false,
                    "finishedAt": "2026-01-15T12:01:00.000Z",
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "partial",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 999,
                                    "output_tokens": 50,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                },
                {
                    "finishedAt": "2026-01-15T12:02:00.000Z",
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "missing-completed",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 100,
                                    "output_tokens": 10,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    assert!(parse_augment_file(f.path()).is_empty());
}

#[test]
fn test_missing_finished_at_falls_back_to_file_mtime() {
    let json = r#"{
            "sessionId": "s-mtime",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                {
                    "completed": true,
                    "exchange": {
                        "model_id": "grok-4-5",
                        "request_id": "r-mtime",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 3,
                                    "output_tokens": 1,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 1);
    let mtime = file_modified_timestamp_ms(f.path());
    // Allow small skew between metadata reads.
    assert!((messages[0].timestamp - mtime).abs() < 5_000);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;
    assert!(messages[0].timestamp > 0);
    assert!(messages[0].timestamp <= now_ms + 5_000);
}

#[test]
fn test_empty_exchange_model_falls_back_then_unknown_provider() {
    let json = r#"{
            "sessionId": "s-unknown",
            "agentState": { "modelId": "   " },
            "chatHistory": [
                {
                    "completed": true,
                    "finishedAt": "2026-07-20T13:33:00.000Z",
                    "exchange": {
                        "model_id": "",
                        "request_id": "r-u",
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 7,
                                    "output_tokens": 1,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "unknown");
    assert_eq!(messages[0].provider_id, "augment");
}

#[test]
fn test_numeric_sequence_id_dedup_key() {
    let json = r#"{
            "sessionId": "s-seq",
            "agentState": { "modelId": "grok-4-5" },
            "chatHistory": [
                {
                    "completed": true,
                    "finishedAt": "2026-07-20T13:33:00.000Z",
                    "sequenceId": 42,
                    "exchange": {
                        "response_nodes": [
                            {
                                "token_usage": {
                                    "input_tokens": 2,
                                    "output_tokens": 1,
                                    "cache_read_input_tokens": 0,
                                    "cache_creation_input_tokens": 0
                                }
                            }
                        ]
                    }
                }
            ]
        }"#;
    let f = write_temp_json(json);
    let messages = parse_augment_file(f.path());
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("augment:s-seq:seq:42")
    );
}
