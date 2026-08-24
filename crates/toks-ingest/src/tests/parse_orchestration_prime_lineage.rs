use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_prime_agent_concurrent_equal_children_are_counted_once() {
    // Two children of the same parent spent identical tokens and finished in
    // the same millisecond, so no timestamp separates one child's response
    // from the other's attribution. Both must still be paired off: keeping
    // the aggregate parent while also counting both transcripts would report
    // their usage twice.
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions = source_home.path().join(".prime/agent/sessions");
    std::fs::create_dir_all(&sessions).unwrap();

    let root_path = sessions.join("a-root.jsonl");
    std::fs::write(
            &root_path,
            r#"{"type":"session","version":3,"id":"root","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":300,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":300}}}
{"type":"child_usage_attributed","id":"usage-a","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":100,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":100},"aggregateUsage":{"input":200,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":200},"origin":"spawn_task"}
{"type":"child_usage_attributed","id":"usage-b","parentId":"usage-a","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":100,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":100},"aggregateUsage":{"input":300,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":300},"origin":"spawn_task"}
"#,
        )
        .unwrap();
    for child in ["sub-a", "sub-b"] {
        let child_dir = source_home
            .path()
            .join(".prime/agent/session-artifacts/a-root")
            .join(child);
        std::fs::create_dir_all(&child_dir).unwrap();
        std::fs::write(
                child_dir.join("child.jsonl"),
                format!(
                    r#"{{"type":"session","version":3,"id":"{child}","timestamp":"2026-08-08T00:00:01.500Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"{child}-response","usage":{{"input":100,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":100}}}}}}
"#,
                    paths::json_path_literal(&root_path)
                ),
            )
            .unwrap();
    }

    let clients = ["prime-agent".to_string()];
    for messages in [
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None),
        // Warm source-cache lane must agree with the cold parse exactly.
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None),
    ] {
        assert_eq!(messages.len(), 3);
        // 100 own parent usage plus the two 100-token children, each counted
        // once from its own transcript.
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.input)
                .sum::<i64>(),
            300
        );
    }

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(source_home.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(clients.to_vec()),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();
    assert_eq!(parsed.messages.len(), 3);
    assert_eq!(
        parsed
            .messages
            .iter()
            .map(|message| message.input)
            .sum::<i64>(),
        300
    );
}

#[test]
#[serial_test::serial]
fn test_prime_agent_colliding_attribution_ids_do_not_cross_lineages() {
    // Prime mints attribution ids as `randomUUID().slice(0, 8)` and only
    // checks them against the session it is writing, so two unrelated
    // sessions can carry the same id. Resolving one lineage's child must
    // not mark the other lineage's attribution as accounted for.
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions = source_home.path().join(".prime/agent/sessions");
    let child_dir = source_home
        .path()
        .join(".prime/agent/session-artifacts/a-lineage/sub-a");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::create_dir_all(&child_dir).unwrap();

    let lineage_a = sessions.join("a-lineage.jsonl");
    std::fs::write(
            &lineage_a,
            r#"{"type":"session","version":3,"id":"parent-a","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-a-response","usage":{"input":120,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":120}}}
{"type":"child_usage_attributed","id":"deadbeef","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":20,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":20},"aggregateUsage":{"input":120,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":120},"origin":"spawn_task"}
"#,
        )
        .unwrap();
    std::fs::write(
            child_dir.join("child.jsonl"),
            format!(
                r#"{{"type":"session","version":3,"id":"child-a","timestamp":"2026-08-08T00:00:01.500Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.001Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-a-response","usage":{{"input":20,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":20}}}}}}
"#,
                paths::json_path_literal(&lineage_a)
            ),
        )
        .unwrap();
    // Same 8-hex id, unrelated session, and its child transcript is gone.
    std::fs::write(
            sessions.join("b-lineage.jsonl"),
            r#"{"type":"session","version":3,"id":"parent-b","timestamp":"2026-08-09T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-09T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-b-response","usage":{"input":130,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":130}}}
{"type":"child_usage_attributed","id":"deadbeef","parentId":"parent","timestamp":"2026-08-09T00:00:02.000Z","targetId":"parent","childUsage":{"input":30,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":30},"aggregateUsage":{"input":130,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":130},"origin":"spawn_task"}
"#,
        )
        .unwrap();

    let clients = ["prime-agent".to_string()];
    for messages in [
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None),
        // Warm source-cache lane must agree with the cold parse exactly.
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None),
    ] {
        assert_eq!(messages.len(), 3);
        // 100 reconciled parent + 20 parsed child + 130 aggregate parent
        // whose own child was pruned.
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.input)
                .sum::<i64>(),
            250
        );
    }

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(source_home.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(clients.to_vec()),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();
    assert_eq!(parsed.messages.len(), 3);
    assert_eq!(
        parsed
            .messages
            .iter()
            .map(|message| message.input)
            .sum::<i64>(),
        250
    );
}

#[test]
#[serial_test::serial]
fn test_prime_agent_contested_child_is_attributed_to_the_nearest_model() {
    // Two parent responses on different models each persist an aggregate that
    // contains one 50-token child, and only the second parent's child
    // transcript survives. Both attributions are inside the tolerance window,
    // so a maximum-cardinality match could reduce either aggregate and leave
    // the global total intact -- but pricing is applied per model after
    // reconciliation, so the wrong choice moves cost between models.
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions = source_home.path().join(".prime/agent/sessions");
    let child_dir = source_home
        .path()
        .join(".prime/agent/session-artifacts/parent/sub-child");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::create_dir_all(&child_dir).unwrap();

    let parent_path = sessions.join("parent.jsonl");
    std::fs::write(
            &parent_path,
            r#"{"type":"session","version":3,"id":"parent","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent-a","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"model-a","responseId":"parent-response-a","usage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}
{"type":"child_usage_attributed","id":"00000000","parentId":"parent-a","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent-a","childUsage":{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50},"aggregateUsage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150},"origin":"spawn_task"}
{"type":"message","id":"parent-b","parentId":"00000000","timestamp":"2026-08-08T00:00:01.500Z","message":{"role":"assistant","provider":"anthropic","model":"model-b","responseId":"parent-response-b","usage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}
{"type":"child_usage_attributed","id":"ffffffff","parentId":"parent-b","timestamp":"2026-08-08T00:00:02.002Z","targetId":"parent-b","childUsage":{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50},"aggregateUsage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150},"origin":"spawn_task"}
"#,
        )
        .unwrap();
    std::fs::write(
            child_dir.join("child.jsonl"),
            format!(
                r#"{{"type":"session","version":3,"id":"child","timestamp":"2026-08-08T00:00:01.600Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.002Z","message":{{"role":"assistant","provider":"anthropic","model":"child-model","responseId":"child-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
"#,
                paths::json_path_literal(&parent_path)
            ),
        )
        .unwrap();

    let clients = ["prime-agent".to_string()];
    for messages in [
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None),
        // The warm source-cache lane must produce the same per-model rows,
        // not just the same total.
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None),
    ] {
        let mut per_model: std::collections::HashMap<String, i64> =
            std::collections::HashMap::new();
        for message in &messages {
            *per_model.entry(message.model_id.clone()).or_default() += message.tokens.input;
        }
        assert_eq!(per_model.get("model-a").copied(), Some(150));
        assert_eq!(per_model.get("model-b").copied(), Some(100));
        assert_eq!(per_model.get("child-model").copied(), Some(50));
        assert_eq!(per_model.values().sum::<i64>(), 300);
    }
}
