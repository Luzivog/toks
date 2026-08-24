use super::*;

#[test]
fn test_parse_kimi_code_valid_usage_record() {
    let content = r#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":5102,"output":172,"inputCacheRead":13312,"inputCacheCreation":0},"usageScope":"turn","time":1780319377014}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].client, "kimi");
    assert_eq!(messages[0].model_id, "kimi-for-coding");
    assert_eq!(messages[0].provider_id, "moonshot");
    assert_eq!(messages[0].session_id, "sess-abc-123");
    assert_eq!(messages[0].tokens.input, 5102);
    assert_eq!(messages[0].tokens.output, 172);
    assert_eq!(messages[0].tokens.cache_read, 13312);
    assert_eq!(messages[0].tokens.cache_write, 0);
    assert_eq!(messages[0].timestamp, 1780319377014);
}

#[test]
fn test_parse_kimi_code_keeps_latest_concrete_model_across_invalid_requests() {
    let content = r#"{"type":"llm.request","model":"k3","time":1780319377000}
{"type":"llm.request","time":1780319377001}
{"type":"llm.request","model":" ","time":1780319377002}
{"type":"llm.request","model":"__runtime_model__","time":1780319377003}
{"type":"llm.request","model":"kimi-code/   ","time":1780319377004}
{"type":"llm.request","model":"kimi-code/ __runtime_model__ ","time":1780319377005}
{"type":"usage.record","model":"kimi-code/__kimi_env_model__","usage":{"inputOther":100,"output":50,"inputCacheRead":25,"inputCacheCreation":0},"usageScope":"turn","time":1780319377010}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].model_id, "k3");
}

#[test]
fn test_parse_kimi_code_prefers_concrete_usage_model_and_tracks_requests() {
    let content = r#"{"type":"llm.request","model":"k3","time":1780319377000}
{"type":"usage.record","model":"__runtime_model__","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377010}
{"type":"llm.request","model":"kimi-code/k3-256k","time":1780319377020}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":200,"output":75,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377030}
{"type":"usage.record","model":"__another_model_alias__","usage":{"inputOther":300,"output":100,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377040}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].model_id, "k3");
    assert_eq!(messages[1].model_id, "kimi-for-coding");
    assert_eq!(messages[2].model_id, "k3-256k");
}

#[test]
fn test_parse_kimi_code_invalid_usage_without_request_uses_default_model() {
    let content = r#"{"type":"usage.record","model":"__kimi_env_model__","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377010}
{"type":"usage.record","model":"kimi-code/__runtime_model__","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377020}
{"type":"usage.record","model":"kimi-code/ __runtime_model__ ","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377030}
{"type":"usage.record","model":"kimi-code/   ","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377040}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 4);
    assert!(messages
        .iter()
        .all(|message| message.model_id == DEFAULT_MODEL));
}

#[test]
fn test_parse_kimi_code_skip_non_usage_record() {
    let content = r#"{"type":"context.append_loop_event","event":{"type":"tool.call","name":"Read"},"time":1780319377000}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319378000}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].timestamp, 1780319378000);
}

#[test]
fn test_parse_kimi_code_non_positive_time_falls_back_to_mtime() {
    let content = r#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":10,"output":1,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":-1500}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":20,"output":2,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":0}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":30,"output":3,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377014}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);
    let mtime = file_modified_timestamp_ms(&fake_path);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].tokens.input, 10);
    assert_eq!(messages[0].timestamp, mtime);
    assert_eq!(messages[1].tokens.input, 20);
    assert_eq!(messages[1].timestamp, mtime);
    assert_eq!(messages[2].tokens.input, 30);
    assert_eq!(messages[2].timestamp, 1780319377014);
}

#[test]
fn test_normalize_kimi_code_model() {
    assert_eq!(
        normalize_kimi_code_model("kimi-code/kimi-for-coding"),
        "kimi-for-coding"
    );
    // No prefix: returned unchanged
    assert_eq!(
        normalize_kimi_code_model("kimi-for-coding"),
        "kimi-for-coding"
    );
    assert_eq!(normalize_kimi_code_model(""), "");
}

#[test]
fn test_parse_kimi_code_session_id_extraction() {
    assert_eq!(
        extract_session_id_from_kimi_code_path(std::path::Path::new(
            "/home/user/.kimi-code/sessions/workspace/session-uuid/agents/main/wire.jsonl"
        )),
        "session-uuid"
    );
    assert_eq!(
        extract_session_id_from_kimi_code_path(std::path::Path::new(
            "C:/Users/Alice/.kimi-code/sessions/workspace/sess-123/agents/coder/wire.jsonl"
        )),
        "sess-123"
    );
    assert_eq!(
        extract_session_id_from_kimi_code_path(std::path::Path::new("wire.jsonl")),
        "unknown"
    );
}

#[test]
fn test_parse_kimi_code_only_counts_turn_scoped_usage() {
    // "session"-scoped records are non-turn bookkeeping (e.g. compaction)
    // and records without usageScope are treated as session-scoped by
    // kimi-code itself; neither should be counted.
    let content = r#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":999,"output":999,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"session","time":1780319377000}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":888,"output":888,"inputCacheRead":0,"inputCacheCreation":0},"time":1780319377005}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":100,"output":50,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377010}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 100);
    assert_eq!(messages[0].tokens.output, 50);
    assert_eq!(messages[0].timestamp, 1780319377010);
}

#[test]
fn test_parse_kimi_code_zero_tokens_skipped() {
    let content = r#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377014}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);
    assert!(messages.is_empty());
}

#[test]
fn test_parse_kimi_code_keeps_extreme_buckets_and_skips_only_all_zero() {
    // MAX + MAX + 2 panics in debug and wraps to zero in release.
    let content = r#"{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":9223372036854775807,"output":9223372036854775807,"inputCacheRead":2,"inputCacheCreation":0},"usageScope":"turn","time":1780319377014}
{"type":"usage.record","model":"kimi-code/kimi-for-coding","usage":{"inputOther":0,"output":0,"inputCacheRead":0,"inputCacheCreation":0},"usageScope":"turn","time":1780319377015}"#;
    let (_dir, fake_path) = create_kimi_code_test_file(content);

    let messages = parse_kimi_code_file(&fake_path);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, i64::MAX);
    assert_eq!(messages[0].tokens.output, i64::MAX);
    assert_eq!(messages[0].tokens.cache_read, 2);
    assert_eq!(messages[0].tokens.cache_write, 0);
}

#[test]
fn test_is_kimi_code_path() {
    assert!(is_kimi_code_path(std::path::Path::new(
        "/home/user/.kimi-code/sessions/workspace/sess/agents/main/wire.jsonl"
    )));
    // Custom KIMI_CODE_HOME root: kimi-code still creates the
    // agents/<AGENT>/wire.jsonl layout underneath it.
    assert!(is_kimi_code_path(std::path::Path::new(
        "/data/kimi/sessions/ws/sess/agents/main/wire.jsonl"
    )));
    assert!(!is_kimi_code_path(std::path::Path::new(
        "/home/user/.kimi/sessions/group/uuid/wire.jsonl"
    )));
    assert!(!is_kimi_code_path(std::path::Path::new("wire.jsonl")));
}
