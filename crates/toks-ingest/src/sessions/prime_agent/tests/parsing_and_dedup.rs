use super::*;

#[test]
fn parses_root_session_without_counting_child_attribution_records() {
    let file = session_file(
        r#"{"type":"session","version":3,"id":"root-1","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"session_info","id":"info","parentId":null,"timestamp":"2026-08-08T00:00:00.500Z","name":"My renamed thread"}
{"type":"message","id":"assistant-1","parentId":"info","timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"msg_provider_001","usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":10,"totalTokens":180}}}
{"type":"child_usage_attributed","id":"usage-1","parentId":"assistant-1","timestamp":"2026-08-08T00:00:02.000Z","targetId":"assistant-1","childUsage":{"input":500,"output":200,"cacheRead":0,"cacheWrite":0,"totalTokens":700},"aggregateUsage":{"input":600,"output":250,"cacheRead":20,"cacheWrite":10,"totalTokens":880},"origin":"spawn_task"}"#,
    );

    let messages = parse_prime_agent_file(file.path());

    assert_eq!(messages.len(), 1);
    let message = &messages[0];
    assert_eq!(message.client, "prime-agent");
    assert_eq!(message.session_id, "root-1");
    assert_eq!(message.workspace_key.as_deref(), Some("/tmp/project"));
    assert_eq!(message.tokens.input, 100);
    assert_eq!(message.tokens.output, 50);
    assert_eq!(message.tokens.cache_read, 20);
    assert_eq!(message.tokens.cache_write, 10);
    assert_eq!(message.agent, None, "a root thread name is not an agent");
    assert_eq!(
        message.dedup_key.as_deref(),
        Some("prime-agent:response:msg_provider_001")
    );
}

#[test]
fn attributes_rlm_child_messages_to_the_session_name() {
    let file = session_file(
        r#"{"type":"session","version":3,"id":"child-1","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","parentSession":"/tmp/root.jsonl","rlmDepth":1}
{"type":"session_info","id":"info","parentId":null,"timestamp":"2026-08-08T00:00:00.500Z","name":"api-reviewer"}
{"type":"message","id":"assistant-1","parentId":"info","timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"openai","model":"gpt-5.4","usage":{"input":40,"output":12,"cacheRead":8,"cacheWrite":0,"totalTokens":60}}}"#,
    );

    let messages = parse_prime_agent_file(file.path());

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].agent.as_deref(), Some("api-reviewer"));
    assert_eq!(messages[0].provider_id, "openai");
    assert_eq!(messages[0].model_id, "gpt-5.4");
}

#[test]
fn keeps_aggregate_parent_when_the_attributed_child_is_unavailable() {
    let file = session_file(
        r#"{"type":"session","version":3,"id":"fork-1","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250}}}
{"type":"child_usage_attributed","id":"usage-1","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":50,"output":20,"cacheRead":0,"cacheWrite":0,"totalTokens":70},"aggregateUsage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250},"origin":"spawn_task"}"#,
    );

    let messages = parse_prime_agent_file(file.path());
    let accounting = analyze_prime_agent_accounting(file.path(), &messages);
    let messages = reconcile_prime_agent_messages(messages, &[accounting]);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 150);
    assert_eq!(messages[0].tokens.output, 70);
    assert_eq!(messages[0].tokens.cache_read, 20);
    assert_eq!(messages[0].tokens.cache_write, 10);
}

#[test]
fn blank_model_message_does_not_shift_accounting_alignment() {
    let file = session_file(
        r#"{"type":"session","version":3,"id":"root","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"blank","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"","responseId":"blank-response","usage":{"input":1,"output":1,"cacheRead":0,"cacheWrite":0,"totalTokens":2}}}
{"type":"message","id":"parent","parentId":"blank","timestamp":"2026-08-08T00:00:02.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250}}}
{"type":"child_usage_attributed","id":"usage-1","parentId":"parent","timestamp":"2026-08-08T00:00:03.000Z","targetId":"parent","childUsage":{"input":50,"output":20,"cacheRead":0,"cacheWrite":0,"totalTokens":70},"aggregateUsage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250},"origin":"spawn_task"}"#,
    );

    let messages = parse_prime_agent_file(file.path());
    let accounting = analyze_prime_agent_accounting(file.path(), &messages);

    assert_eq!(messages.len(), 2);
    assert_eq!(accounting.adjustments.len(), 1);
    assert_eq!(
        accounting.adjustments[0].dedup_key,
        "prime-agent:response:parent-response"
    );
}

#[test]
fn sibling_forks_preserve_each_distinct_unavailable_child_delta() {
    fn tokens(input: i64) -> TokenBreakdown {
        TokenBreakdown {
            input,
            ..TokenBreakdown::default()
        }
    }
    fn parent_message(input: i64, session: &str) -> UnifiedMessage {
        let mut message = UnifiedMessage::new(
            "prime-agent",
            "claude-opus-5",
            "anthropic",
            session,
            1,
            tokens(input),
            0.0,
        );
        message.dedup_key = Some("prime-agent:response:shared-parent".to_string());
        message
    }
    fn fork_accounting(
        source: &str,
        attribution_id: &str,
        child_input: i64,
    ) -> PrimeFileAccounting {
        let attribution = PrimeAttribution {
            id: attribution_id.to_string(),
            timestamp: Some(1),
            child_usage: tokens(child_input),
            aggregate_usage: tokens(100 + child_input),
        };
        PrimeFileAccounting {
            source_path: PathBuf::from(source),
            attributions: vec![attribution.clone()],
            adjustments: vec![PrimeUsageAdjustment {
                dedup_key: "prime-agent:response:shared-parent".to_string(),
                persisted_usage: tokens(100 + child_input),
                attributions: vec![attribution],
            }],
            ..PrimeFileAccounting::default()
        }
    }

    let messages = vec![parent_message(150, "fork-a"), parent_message(130, "fork-b")];
    let accounting = [
        fork_accounting("fork-a.jsonl", "child-a", 50),
        fork_accounting("fork-b.jsonl", "child-b", 30),
    ];
    let messages = reconcile_prime_agent_messages(messages, &accounting);

    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].tokens.input, 180);
}

#[test]
fn equal_child_usage_is_matched_by_parent_lineage_and_completion_time() {
    let dir = tempfile::TempDir::new().unwrap();
    let parent_path = dir.path().join("parent.jsonl");
    let child_path = dir.path().join("child.jsonl");
    std::fs::write(
        &parent_path,
        r#"{"type":"session","version":3,"id":"parent","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent-a","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"model-a","responseId":"parent-response-a","usage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}
{"type":"child_usage_attributed","id":"usage-a","parentId":"parent-a","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent-a","childUsage":{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50},"aggregateUsage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150},"origin":"spawn_task"}
{"type":"message","id":"parent-b","parentId":"usage-a","timestamp":"2026-08-08T00:00:10.000Z","message":{"role":"assistant","provider":"anthropic","model":"model-b","responseId":"parent-response-b","usage":{"input":250,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":250}}}
{"type":"child_usage_attributed","id":"usage-b","parentId":"parent-b","timestamp":"2026-08-08T00:00:11.000Z","targetId":"parent-b","childUsage":{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50},"aggregateUsage":{"input":250,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":250},"origin":"spawn_task"}
"#,
    )
    .unwrap();
    std::fs::write(
        &child_path,
        format!(
            r#"{{"type":"session","version":3,"id":"child","timestamp":"2026-08-08T00:00:10.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:11.001Z","message":{{"role":"assistant","provider":"anthropic","model":"child-model","responseId":"child-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
"#,
            serde_json::to_string(&parent_path.to_string_lossy()).unwrap()
        ),
    )
    .unwrap();

    let parent_messages = parse_prime_agent_file(&parent_path);
    let child_messages = parse_prime_agent_file(&child_path);
    let accounting = [
        analyze_prime_agent_accounting(&parent_path, &parent_messages),
        analyze_prime_agent_accounting(&child_path, &child_messages),
    ];
    let messages = reconcile_prime_agent_messages(
        parent_messages.into_iter().chain(child_messages).collect(),
        &accounting,
    );

    let parent_a = messages
        .iter()
        .find(|message| {
            message.dedup_key.as_deref() == Some("prime-agent:response:parent-response-a")
        })
        .unwrap();
    let parent_b = messages
        .iter()
        .find(|message| {
            message.dedup_key.as_deref() == Some("prime-agent:response:parent-response-b")
        })
        .unwrap();
    assert_eq!(parent_a.tokens.input, 150);
    assert_eq!(parent_b.tokens.input, 200);
}

#[test]
fn same_sized_child_from_another_parent_does_not_authorize_subtraction() {
    let dir = tempfile::TempDir::new().unwrap();
    let parent_path = dir.path().join("parent-a.jsonl");
    let child_path = dir.path().join("child-b.jsonl");
    let unrelated_parent = dir.path().join("parent-b.jsonl");
    std::fs::write(
        &parent_path,
        r#"{"type":"session","version":3,"id":"parent-a","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250}}}
{"type":"child_usage_attributed","id":"usage-a","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":50,"output":20,"cacheRead":0,"cacheWrite":0,"totalTokens":70},"aggregateUsage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250},"origin":"spawn_task"}
"#,
    )
    .unwrap();
    std::fs::write(
        &child_path,
        format!(
            r#"{{"type":"session","version":3,"id":"child-b","timestamp":"2026-08-08T00:00:01.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child","parentId":null,"timestamp":"2026-08-08T00:00:02.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-response","usage":{{"input":50,"output":20,"cacheRead":0,"cacheWrite":0,"totalTokens":70}}}}}}
"#,
            serde_json::to_string(&unrelated_parent.to_string_lossy()).unwrap()
        ),
    )
    .unwrap();

    let parent_messages = parse_prime_agent_file(&parent_path);
    let child_messages = parse_prime_agent_file(&child_path);
    let accounting = [
        analyze_prime_agent_accounting(&parent_path, &parent_messages),
        analyze_prime_agent_accounting(&child_path, &child_messages),
    ];
    let messages = reconcile_prime_agent_messages(
        parent_messages.into_iter().chain(child_messages).collect(),
        &accounting,
    );

    assert_eq!(messages.len(), 2);
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.input)
            .sum::<i64>(),
        200
    );
    assert_eq!(
        messages
            .iter()
            .map(|message| message.tokens.output)
            .sum::<i64>(),
        90
    );
}

#[test]
fn copied_fork_history_keeps_a_cross_session_dedup_key() {
    let original = session_file(
        r#"{"type":"session","version":3,"id":"root-1","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"assistant-1","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"msg_provider_001","usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":10,"totalTokens":180}}}"#,
    );
    let fork = session_file(
        r#"{"type":"session","version":3,"id":"fork-2","timestamp":"2026-08-08T01:00:00.000Z","cwd":"/tmp/project","parentSession":"/tmp/root.jsonl","rlmDepth":0}
{"type":"message","id":"assistant-1","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"msg_provider_001","usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":10,"totalTokens":180}}}"#,
    );

    let original = parse_prime_agent_file(original.path());
    let fork = parse_prime_agent_file(fork.path());

    assert_eq!(original.len(), 1);
    assert_eq!(fork.len(), 1);
    assert_eq!(original[0].dedup_key, fork[0].dedup_key);
}

#[test]
fn copied_fork_history_without_response_or_event_timestamp_still_deduplicates() {
    let original = session_file(
        r#"{"type":"session","version":3,"id":"root-1","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"assistant-1","parentId":null,"message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":10,"totalTokens":180}}}"#,
    );
    std::thread::sleep(std::time::Duration::from_millis(10));
    let fork = session_file(
        r#"{"type":"session","version":3,"id":"fork-2","timestamp":"2026-08-08T01:00:00.000Z","cwd":"/tmp/project","parentSession":"/tmp/root.jsonl","rlmDepth":0}
{"type":"message","id":"assistant-1","parentId":null,"message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":10,"totalTokens":180}}}"#,
    );

    let original = parse_prime_agent_file(original.path());
    let fork = parse_prime_agent_file(fork.path());

    assert_ne!(original[0].timestamp, fork[0].timestamp);
    assert_eq!(original[0].dedup_key, fork[0].dedup_key);
}
