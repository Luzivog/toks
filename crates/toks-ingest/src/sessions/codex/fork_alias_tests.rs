use super::parse_codex_file;
use std::path::Path;

fn write_child(path: &Path, child_id: &str, child_turn_id: &str, timestamp: &str) {
    std::fs::write(
        path,
        format!(
            concat!(
                r#"{{"timestamp":"{timestamp}","type":"session_meta","payload":{{"id":"{child_id}","forked_from_id":"019e5b00-0000-7000-8000-000000000001","source":{{"subagent":{{"thread_spawn":{{"parent_thread_id":"019e5b00-0000-7000-8000-000000000001","depth":1}}}}}},"model_provider":"openai","cwd":"/repo"}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"session_meta","payload":{{"id":"019e5b00-0000-7000-8000-000000000001","source":"vscode","model_provider":"openai","cwd":"/repo"}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"turn_context","payload":{{"turn_id":"019e5b00-0001-7000-8000-000000000001","model":"gpt-5.5"}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":100,"output_tokens":10}},"last_token_usage":{{"input_tokens":100,"output_tokens":10}}}}}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":130,"output_tokens":13}},"last_token_usage":{{"input_tokens":30,"output_tokens":3}}}}}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"task_started","turn_id":"{child_turn_id}"}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"turn_context","payload":{{"turn_id":"{child_turn_id}","model":"gpt-5.5"}}}}"#,
                "\n",
                r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":140,"output_tokens":14}},"last_token_usage":{{"input_tokens":10,"output_tokens":1}}}}}}}}"#,
                "\n"
            ),
            timestamp = timestamp,
            child_id = child_id,
            child_turn_id = child_turn_id,
        ),
    )
    .unwrap();
}

#[test]
fn fork_sibling_representatives_share_alias_not_primary_identity() {
    let source_dir = tempfile::TempDir::new().unwrap();
    let child_a_path = source_dir.path().join("child-a.jsonl");
    let child_b_path = source_dir.path().join("child-b.jsonl");
    write_child(
        &child_a_path,
        "019e5c03-1e99-7000-8000-000000000001",
        "019e5c03-6425-7000-8000-000000000001",
        "2026-05-24T21:00:00Z",
    );
    write_child(
        &child_b_path,
        "019e5c04-1e99-7000-8000-000000000001",
        "019e5c04-6425-7000-8000-000000000001",
        "2026-05-24T22:00:00Z",
    );

    let child_a = parse_codex_file(&child_a_path);
    let child_b = parse_codex_file(&child_b_path);
    assert_eq!(child_a.len(), 1);
    assert_eq!(child_b.len(), 1);
    assert_ne!(child_a[0].durable_identity, child_b[0].durable_identity);
    assert_eq!(child_a[0].accounting_aliases.len(), 1);
    assert_eq!(child_a[0].accounting_aliases, child_b[0].accounting_aliases);

    std::fs::remove_file(child_a_path).unwrap();
    let remaining_child_b = parse_codex_file(&child_b_path);
    assert_eq!(
        child_a[0].accounting_aliases,
        remaining_child_b[0].accounting_aliases
    );
}
