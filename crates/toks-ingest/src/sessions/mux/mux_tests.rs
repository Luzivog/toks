use super::*;
use std::io::Write;
use tempfile::NamedTempFile;

fn write_temp_json(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    f.flush().unwrap();
    f
}

#[test]
fn test_parse_valid_session_usage() {
    let json = r#"{
            "version": 1,
            "byModel": {
                "anthropic:claude-opus-4-6": {
                    "input": { "tokens": 100, "cost_usd": 0.01 },
                    "cached": { "tokens": 5000, "cost_usd": 0.05 },
                    "cacheCreate": { "tokens": 200, "cost_usd": 0.02 },
                    "output": { "tokens": 300, "cost_usd": 0.03 },
                    "reasoning": { "tokens": 0, "cost_usd": 0 }
                },
                "openai:gpt-4o": {
                    "input": { "tokens": 50, "cost_usd": 0.005 },
                    "cached": { "tokens": 0, "cost_usd": 0 },
                    "cacheCreate": { "tokens": 0, "cost_usd": 0 },
                    "output": { "tokens": 150, "cost_usd": 0.015 },
                    "reasoning": { "tokens": 0, "cost_usd": 0 }
                }
            },
            "lastRequest": {
                "model": "anthropic:claude-opus-4-6",
                "timestamp": 1700000000000
            }
        }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert_eq!(msgs.len(), 2);

    // Find the claude message
    let claude = msgs
        .iter()
        .find(|m| m.model_id == "claude-opus-4-6")
        .unwrap();
    assert_eq!(claude.client, "mux");
    assert_eq!(claude.provider_id, "anthropic");
    assert_eq!(claude.tokens.input, 100);
    assert_eq!(claude.tokens.cache_read, 5000);
    assert_eq!(claude.tokens.cache_write, 200);
    assert_eq!(claude.tokens.output, 300);
    assert_eq!(claude.tokens.reasoning, 0);
    assert_eq!(claude.timestamp, 1700000000000);

    let gpt = msgs.iter().find(|m| m.model_id == "gpt-4o").unwrap();
    assert_eq!(gpt.provider_id, "openai");
    assert_eq!(gpt.tokens.input, 50);
    assert_eq!(gpt.tokens.output, 150);
}

#[test]
fn test_parse_empty_by_model() {
    let json = r#"{ "version": 1, "byModel": {} }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_parse_missing_by_model() {
    let json = r#"{ "version": 1 }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_zero_token_entries_filtered() {
    let json = r#"{
            "version": 1,
            "byModel": {
                "anthropic:claude-opus-4-6": {
                    "input": { "tokens": 0, "cost_usd": 0 },
                    "cached": { "tokens": 0, "cost_usd": 0 },
                    "cacheCreate": { "tokens": 0, "cost_usd": 0 },
                    "output": { "tokens": 0, "cost_usd": 0 },
                    "reasoning": { "tokens": 0, "cost_usd": 0 }
                }
            },
            "lastRequest": { "model": "anthropic:claude-opus-4-6", "timestamp": 1700000000000 }
        }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_model_without_provider_prefix() {
    let json = r#"{
            "version": 1,
            "byModel": {
                "claude-opus-4-6": {
                    "input": { "tokens": 100 },
                    "output": { "tokens": 200 }
                }
            },
            "lastRequest": { "timestamp": 1700000000000 }
        }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].model_id, "claude-opus-4-6");
    assert_eq!(msgs[0].provider_id, "");
}

#[test]
fn test_invalid_json() {
    let f = write_temp_json("not json at all");
    let msgs = parse_mux_file(f.path());
    assert!(msgs.is_empty());
}

#[test]
fn test_nonexistent_file() {
    let msgs = parse_mux_file(Path::new("/nonexistent/path/session-usage.json"));
    assert!(msgs.is_empty());
}

#[test]
fn test_negative_tokens_clamped() {
    let json = r#"{
            "version": 1,
            "byModel": {
                "anthropic:claude-opus-4-6": {
                    "input": { "tokens": -50, "cost_usd": 0.01 },
                    "output": { "tokens": 100, "cost_usd": 0.02 }
                }
            },
            "lastRequest": { "timestamp": 1700000000000 }
        }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].tokens.input, 0);
    assert_eq!(msgs[0].tokens.output, 100);
}

#[test]
fn test_source_cost_summed() {
    let json = r#"{
            "version": 1,
            "byModel": {
                "anthropic:claude-opus-4-6": {
                    "input": { "tokens": 100, "cost_usd": 0.01 },
                    "cached": { "tokens": 200, "cost_usd": 0.02 },
                    "cacheCreate": { "tokens": 50, "cost_usd": 0.005 },
                    "output": { "tokens": 300, "cost_usd": 0.03 },
                    "reasoning": { "tokens": 0, "cost_usd": 0 }
                }
            },
            "lastRequest": { "timestamp": 1700000000000 }
        }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert_eq!(msgs.len(), 1);
    let expected_cost = 0.01 + 0.02 + 0.005 + 0.03;
    assert!((msgs[0].cost - expected_cost).abs() < 1e-10);
}

#[test]
fn test_multi_colon_model_key() {
    let json = r#"{
            "version": 1,
            "byModel": {
                "provider:sub:model-name": {
                    "input": { "tokens": 100 },
                    "output": { "tokens": 200 }
                }
            },
            "lastRequest": { "timestamp": 1700000000000 }
        }"#;
    let f = write_temp_json(json);
    let msgs = parse_mux_file(f.path());
    assert_eq!(msgs.len(), 1);
    assert_eq!(msgs[0].provider_id, "provider");
    assert_eq!(msgs[0].model_id, "sub:model-name");
}

#[test]
fn test_dedup_key_distinct_across_workspaces() {
    // Two mux workspaces that recorded the same model must produce distinct
    // dedup keys. The positional key `mux:<model>:<index>` gave the first
    // model in every workspace file `mux:<model>:0`, so two
    // `.mux/sessions/<workspaceId>/session-usage.json` files with the same
    // model collided in a cross-file dedup set and one workspace's usage was
    // dropped. The index is also the HashMap iteration position, unstable
    // across re-parses. Keying on the workspace id (the file's parent
    // directory) is both unique per workspace and stable across re-parses.
    let dir = tempfile::tempdir().unwrap();
    let json = r#"{
            "version": 1,
            "byModel": {
                "anthropic:claude-opus-4-6": {
                    "input": { "tokens": 100 },
                    "output": { "tokens": 200 }
                }
            },
            "lastRequest": { "timestamp": 1700000000000 }
        }"#;

    let write_workspace = |workspace: &str| {
        let workspace_dir = dir.path().join(workspace);
        std::fs::create_dir_all(&workspace_dir).unwrap();
        let file = workspace_dir.join("session-usage.json");
        std::fs::write(&file, json).unwrap();
        file
    };

    let alpha = write_workspace("ws_alpha");
    let beta = write_workspace("ws_beta");

    let alpha_key = parse_mux_file(&alpha)[0].dedup_key.clone();
    let beta_key = parse_mux_file(&beta)[0].dedup_key.clone();

    assert!(alpha_key.is_some());
    assert_ne!(
        alpha_key, beta_key,
        "same model in two workspaces must not share a dedup key"
    );

    // Re-parsing the same workspace file yields a stable key.
    assert_eq!(alpha_key, parse_mux_file(&alpha)[0].dedup_key);
}
