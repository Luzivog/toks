use super::*;
use std::io::Write;

fn write_fixture(data: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(data.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn test_parse_empty_file() {
    let f = write_fixture("[]");
    let msgs = parse_trae_file("trae", f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_parse_single_session() {
    let json = serde_json::json!([{
        "model_name": "GPT-5.4",
        "session_id": "test-session-1",
        "usage_time": 1776000000,
        "dollar_float": 0.5,
        "extra_info": {
            "input_token": 1000,
            "output_token": 500,
            "cache_read_token": 200,
            "cache_write_token": 100
        }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert_eq!(msgs.len(), 1);
    let m = &msgs[0];
    assert_eq!(m.client, "trae");
    assert_eq!(m.model_id, "gpt-5.4");
    assert_eq!(m.provider_id, "openai");
    assert_eq!(m.tokens.input, 1000);
    assert_eq!(m.tokens.output, 500);
    assert_eq!(m.tokens.cache_read, 200);
    assert_eq!(m.tokens.cache_write, 100);
    assert_eq!(m.cost, 0.5);
    // timestamp: epoch seconds → ms
    assert_eq!(m.timestamp, 1_776_000_000_000);
}

#[test]
fn test_skip_zero_token_session() {
    let json = serde_json::json!([{
        "model_name": "GPT-5.4",
        "session_id": "empty-session",
        "usage_time": 1776000000,
        "dollar_float": 0.0,
        "extra_info": {
            "input_token": 0,
            "output_token": 0,
            "cache_read_token": 0,
            "cache_write_token": 0
        }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_normalize_model_names() {
    assert_eq!(normalize_trae_model("GPT-5.4"), "gpt-5.4");
    assert_eq!(normalize_trae_model("GPT-5.3-Codex"), "gpt-5.3-codex");
    assert_eq!(normalize_trae_model("GPT-5.3 Codex"), "gpt-5.3-codex");
    assert_eq!(normalize_trae_model("Gemini 3.1 Pro"), "gemini-3.1-pro");
    assert_eq!(normalize_trae_model("GLM 5.1"), "glm-5.1");
    assert_eq!(normalize_trae_model("Unknown Model"), "Unknown Model");
}

#[test]
fn test_provider_mapping() {
    let provider_for = |model| provider_from_model_or(model, "trae");
    assert_eq!(provider_for("GPT-5.4"), "openai");
    assert_eq!(provider_for("Claude Sonnet 4.6"), "anthropic");
    assert_eq!(provider_for("Gemini 3.1 Pro"), "google");
    assert_eq!(provider_for("GLM 5.1"), "zai");
    assert_eq!(provider_for("SomeOtherModel"), "trae");
}

#[test]
fn test_auto_mode_fallback_uses_mode_as_model() {
    // Trae's "Auto" mode returns `model_name: ""` because no single
    // model is bound to the session. The parser must still keep the
    // cost and bucket it under `trae-auto`.
    let json = serde_json::json!([{
        "model_name": "",
        "mode": "Auto",
        "session_id": "auto-session-1",
        "usage_time": 1776000000,
        "dollar_float": 0.27,
        "extra_info": {
            "input_token": 159213,
            "output_token": 210,
            "cache_read_token": 6144,
            "cache_write_token": 0
        }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert_eq!(msgs.len(), 1);
    let m = &msgs[0];
    assert_eq!(m.model_id, "trae-auto");
    assert_eq!(m.provider_id, "trae");
    assert_eq!(m.cost, 0.27);
}

#[test]
fn test_skip_session_without_session_id() {
    // A record without `session_id` would otherwise dedup to the same
    // key as every other malformed record. Drop it instead.
    let json = serde_json::json!([{
        "model_name": "GPT-5.4",
        "usage_time": 1776000000,
        "dollar_float": 0.1,
        "extra_info": { "input_token": 100, "output_token": 1, "cache_read_token": 0, "cache_write_token": 0 }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_skip_session_without_usage_time() {
    // No `usage_time` → would land at epoch 0. Drop it.
    let json = serde_json::json!([{
        "model_name": "GPT-5.4",
        "session_id": "abc",
        "dollar_float": 0.1,
        "extra_info": { "input_token": 100, "output_token": 1, "cache_read_token": 0, "cache_write_token": 0 }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_skip_session_with_non_positive_usage_time() {
    let json = serde_json::json!([{
        "model_name": "GPT-5.4",
        "session_id": "abc",
        "usage_time": 0,
        "dollar_float": 0.1,
        "extra_info": { "input_token": 100, "output_token": 1, "cache_read_token": 0, "cache_write_token": 0 }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_skip_session_with_overflowing_usage_time() {
    // A maliciously crafted cache could contain a near-MAX `usage_time`.
    // Multiplying by 1000 would overflow `i64` — debug-panic or wrap to
    // a negative timestamp. Reject the record instead.
    let json = serde_json::json!([{
        "model_name": "GPT-5.4",
        "session_id": "evil",
        "usage_time": i64::MAX,
        "dollar_float": 0.1,
        "extra_info": { "input_token": 100, "output_token": 1, "cache_read_token": 0, "cache_write_token": 0 }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_missing_model_and_mode_falls_back_to_unknown() {
    let json = serde_json::json!([{
        "session_id": "no-meta",
        "usage_time": 1776000000,
        "dollar_float": 0.01,
        "extra_info": { "input_token": 100, "output_token": 1, "cache_read_token": 0, "cache_write_token": 0 }
    }]);
    let f = write_fixture(&json.to_string());
    let msgs = parse_trae_file("trae", f.path());
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].model_id, "trae-unknown");
}
