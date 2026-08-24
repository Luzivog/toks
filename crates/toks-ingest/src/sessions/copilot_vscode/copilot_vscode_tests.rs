use super::*;
use std::io::Write;

fn write_jsonl(path: &Path, lines: &[&str]) {
    let mut f = std::fs::File::create(path).unwrap();
    for line in lines {
        writeln!(f, "{}", line).unwrap();
    }
}

#[test]
fn parse_kind0_with_requests() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let uuid = "550e8400-e29b-41d4-a716-446655440000";
    let path = sessions_dir.join(format!("{}.jsonl", uuid));

    write_jsonl(
        &path,
        &[
            r#"{"kind":0,"v":{"requests":[{"requestId":"r1","timestamp":1783918304896,"modelId":"copilot/auto","completionTokens":154,"promptTokens":22079,"result":{"metadata":{"promptTokens":22079,"outputTokens":154,"resolvedModel":"gpt-5.3-codex"}}}]}}"#,
        ],
    );

    let messages = parse_copilot_vscode_sessions(&[path]);
    assert_eq!(messages.len(), 1);
    let m = &messages[0];
    assert_eq!(m.client, "copilot");
    assert_eq!(m.session_id, uuid);
    assert_eq!(m.model_id, "gpt-5.3-codex");
    assert_eq!(m.timestamp, 1783918304896);
    assert_eq!(m.tokens.input, 22079);
    assert_eq!(m.tokens.output, 154);
    assert_eq!(m.tokens.reasoning, 0);
    assert_eq!(
        m.dedup_key.as_deref(),
        Some(format!("copilot-vscode:{}:1783918304896", uuid).as_str())
    );
}

#[test]
fn parse_kind2_array_append() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let uuid = "650e8400-e29b-41d4-a716-446655440001";
    let path = sessions_dir.join(format!("{}.jsonl", uuid));

    write_jsonl(
        &path,
        &[
            r#"{"kind":0,"v":{"requests":[]}}"#,
            r#"{"kind":2,"k":["requests"],"v":[{"requestId":"r2","timestamp":1783918310000,"modelId":"copilot/auto","completionTokens":200,"promptTokens":5000,"result":{"metadata":{"promptTokens":5000,"outputTokens":200,"resolvedModel":"gpt-5.3-codex","toolCallRounds":[{"thinking":{"tokens":88}},{"thinking":{"tokens":12}}]}}}]}"#,
        ],
    );

    let messages = parse_copilot_vscode_sessions(&[path]);
    assert_eq!(messages.len(), 1);
    let m = &messages[0];
    assert_eq!(m.tokens.input, 5000);
    assert_eq!(m.tokens.output, 200);
    assert_eq!(m.tokens.reasoning, 100);
}

#[test]
fn keeps_parsing_requests_after_an_undecodable_line() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let path = sessions_dir.join("cccccccc-0000-0000-0000-000000000000.jsonl");

    let mut fixture = Vec::new();
    fixture.extend_from_slice(br#"{"kind":0,"v":{"requests":[]}}"#);
    fixture.push(b'\n');
    // A lone 0xff can never appear in valid UTF-8, so `BufRead::lines()`
    // reports this line as `InvalidData`.
    fixture.extend_from_slice(b"{\"kind\":9,\"v\":\"\xff\xfe\"}\n");
    fixture.extend_from_slice(
            br#"{"kind":2,"k":["requests"],"v":[{"requestId":"r9","timestamp":1783918310000,"modelId":"copilot/auto","completionTokens":200,"promptTokens":5000,"result":{"metadata":{"promptTokens":5000,"outputTokens":200,"resolvedModel":"gpt-5.3-codex"}}}]}"#,
        );
    fixture.push(b'\n');
    std::fs::write(&path, &fixture).unwrap();

    let messages = parse_copilot_vscode_sessions(&[path]);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 5000);
}

#[test]
fn skips_zero_token_requests() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let path = sessions_dir.join("aaaaaaaa-0000-0000-0000-000000000000.jsonl");

    write_jsonl(
        &path,
        &[
            r#"{"kind":2,"k":["requests"],"v":[{"requestId":"r0","timestamp":1000,"modelId":"copilot/auto","completionTokens":0,"promptTokens":0}]}"#,
        ],
    );

    assert!(parse_copilot_vscode_sessions(&[path]).is_empty());
}

#[test]
fn model_fallback_from_model_id() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let path = sessions_dir.join("bbbbbbbb-0000-0000-0000-000000000000.jsonl");

    // No resolvedModel, only modelId with "copilot/" prefix
    write_jsonl(
        &path,
        &[
            r#"{"kind":2,"k":["requests"],"v":[{"requestId":"r3","timestamp":2000,"modelId":"copilot/gpt-4o","completionTokens":50,"promptTokens":300}]}"#,
        ],
    );

    let messages = parse_copilot_vscode_sessions(&[path]);
    assert_eq!(messages.len(), 1);
    // "copilot/" prefix stripped
    assert_eq!(messages[0].model_id, "gpt-4o");
}

#[test]
fn reasoning_tokens_summed_from_tool_call_rounds() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let path = sessions_dir.join("cccccccc-0000-0000-0000-000000000000.jsonl");

    write_jsonl(
        &path,
        &[
            r#"{"kind":2,"k":["requests"],"v":[{"requestId":"r4","timestamp":3000,"modelId":"copilot/auto","completionTokens":10,"promptTokens":100,"result":{"metadata":{"resolvedModel":"gpt-5.3-codex","toolCallRounds":[{"thinking":{"tokens":30}},{"thinking":{"tokens":70}}]}}}]}"#,
        ],
    );

    let messages = parse_copilot_vscode_sessions(&[path]);
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.reasoning, 100);
}

#[test]
fn non_copilot_model_id_skipped() {
    let dir = tempfile::tempdir().unwrap();
    let sessions_dir = dir.path().join("chatSessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let path = sessions_dir.join("dddddddd-0000-0000-0000-000000000000.jsonl");

    // modelId does not start with "copilot/" and no resolvedModel
    write_jsonl(
        &path,
        &[
            r#"{"kind":2,"k":["requests"],"v":[{"requestId":"r5","timestamp":4000,"modelId":"some-other-extension/model","completionTokens":50,"promptTokens":300}]}"#,
        ],
    );

    assert!(parse_copilot_vscode_sessions(&[path]).is_empty());
}
