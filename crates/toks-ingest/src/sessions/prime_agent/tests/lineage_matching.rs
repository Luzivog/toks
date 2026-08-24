use super::*;

/// Prime allocates attribution ids with `randomUUID().slice(0, 8)` and only
/// collision-checks them against the current session's own id map, so the
/// same 32-bit id can appear in two unrelated sessions. A collision must not
/// let one lineage's parsed child authorize a subtraction in the other.
#[test]
fn colliding_attribution_ids_in_separate_lineages_stay_independent() {
    fn totals(reverse: bool) -> (i64, i64) {
        let dir = tempfile::TempDir::new().unwrap();
        let parent_a = dir.path().join("parent-a.jsonl");
        let child_a = dir.path().join("child-a.jsonl");
        let parent_b = dir.path().join("parent-b.jsonl");
        std::fs::write(
            &parent_a,
            r#"{"type":"session","version":3,"id":"parent-a","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-a-response","usage":{"input":120,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":120}}}
{"type":"child_usage_attributed","id":"deadbeef","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":20,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":20},"aggregateUsage":{"input":120,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":120},"origin":"spawn_task"}
"#,
        )
        .unwrap();
        std::fs::write(
            &child_a,
            format!(
                r#"{{"type":"session","version":3,"id":"child-a","timestamp":"2026-08-08T00:00:01.500Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child","parentId":null,"timestamp":"2026-08-08T00:00:02.001Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-a-response","usage":{{"input":20,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":20}}}}}}
"#,
                serde_json::to_string(&parent_a.to_string_lossy()).unwrap()
            ),
        )
        .unwrap();
        // Unrelated lineage reusing the same 8-hex attribution id. Its own
        // child transcript was pruned, so the aggregate parent must stand.
        std::fs::write(
            &parent_b,
            r#"{"type":"session","version":3,"id":"parent-b","timestamp":"2026-08-09T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-09T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-b-response","usage":{"input":130,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":130}}}
{"type":"child_usage_attributed","id":"deadbeef","parentId":"parent","timestamp":"2026-08-09T00:00:02.000Z","targetId":"parent","childUsage":{"input":30,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":30},"aggregateUsage":{"input":130,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":130},"origin":"spawn_task"}
"#,
        )
        .unwrap();

        let mut paths = vec![parent_a, child_a, parent_b];
        if reverse {
            paths.reverse();
        }
        let parsed: Vec<(PathBuf, Vec<UnifiedMessage>)> = paths
            .into_iter()
            .map(|path| {
                let messages = parse_prime_agent_file(&path);
                (path, messages)
            })
            .collect();
        let accounting: Vec<PrimeFileAccounting> = parsed
            .iter()
            .map(|(path, messages)| analyze_prime_agent_accounting(path, messages))
            .collect();
        let messages: Vec<UnifiedMessage> = parsed
            .into_iter()
            .flat_map(|(_, messages)| messages)
            .collect();
        let messages = reconcile_prime_agent_messages(messages, &accounting);

        let parent_b_input = messages
            .iter()
            .find(|message| {
                message.dedup_key.as_deref() == Some("prime-agent:response:parent-b-response")
            })
            .map_or(0, |message| message.tokens.input);
        (
            messages.iter().map(|message| message.tokens.input).sum(),
            parent_b_input,
        )
    }

    // parent-a reconciles to 100, its parsed child contributes 20, and the
    // pruned lineage keeps its full aggregate 130.
    assert_eq!(totals(false), (250, 130));
    assert_eq!(totals(true), (250, 130));
}

#[test]
fn attributed_child_larger_than_the_parent_aggregate_clamps_at_zero() {
    fn tokens(input: i64, output: i64) -> TokenBreakdown {
        TokenBreakdown {
            input,
            output,
            ..TokenBreakdown::default()
        }
    }

    let mut message = UnifiedMessage::new(
        "prime-agent",
        "claude-opus-5",
        "anthropic",
        "partial",
        1,
        tokens(40, 10),
        0.0,
    );
    message.dedup_key = Some("prime-agent:response:partial".to_string());
    let attribution = PrimeAttribution {
        id: "deadbeef".to_string(),
        timestamp: Some(1),
        child_usage: tokens(90, 25),
        aggregate_usage: tokens(40, 10),
    };
    let accounting = [PrimeFileAccounting {
        source_path: PathBuf::from("partial.jsonl"),
        attributions: vec![attribution.clone()],
        adjustments: vec![PrimeUsageAdjustment {
            dedup_key: "prime-agent:response:partial".to_string(),
            persisted_usage: tokens(40, 10),
            attributions: vec![attribution],
        }],
        ..PrimeFileAccounting::default()
    }];

    let messages = reconcile_prime_agent_messages(vec![message], &accounting);

    assert_eq!(messages.len(), 1);
    // 40 - 90 clamps to 0 instead of wrapping, then the unavailable child is
    // restored, so the row never reports a negative or absurd bucket.
    assert_eq!(messages[0].tokens.input, 90);
    assert_eq!(messages[0].tokens.output, 25);
}

/// Two child responses of the same size completing inside one timestamp
/// millisecond used to produce two tied candidates for each attribution.
/// Rejecting both ties left the parent aggregate holding both children while
/// the two child transcripts were also counted, double counting them.
#[test]
fn concurrent_equal_sized_children_pair_off_with_their_attributions() {
    fn totals(reverse: bool) -> (i64, i64) {
        let dir = tempfile::TempDir::new().unwrap();
        let parent_path = dir.path().join("parent.jsonl");
        let child_a = dir.path().join("child-a.jsonl");
        let child_b = dir.path().join("child-b.jsonl");
        std::fs::write(
            &parent_path,
            r#"{"type":"session","version":3,"id":"parent","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":300,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":300}}}
{"type":"child_usage_attributed","id":"usage-a","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":100,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":100},"aggregateUsage":{"input":200,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":200},"origin":"spawn_task"}
{"type":"child_usage_attributed","id":"usage-b","parentId":"usage-a","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":100,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":100},"aggregateUsage":{"input":300,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":300},"origin":"spawn_task"}
"#,
        )
        .unwrap();
        // Both children answered the same parent in the same millisecond, so
        // neither timestamp distinguishes them from the other's attribution.
        for (path, response) in [
            (&child_a, "child-a-response"),
            (&child_b, "child-b-response"),
        ] {
            std::fs::write(
                path,
                format!(
                    r#"{{"type":"session","version":3,"id":"{response}","timestamp":"2026-08-08T00:00:01.500Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"{response}","usage":{{"input":100,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":100}}}}}}
"#,
                    serde_json::to_string(&parent_path.to_string_lossy()).unwrap()
                ),
            )
            .unwrap();
        }

        let mut paths = vec![parent_path, child_a, child_b];
        if reverse {
            paths.reverse();
        }
        let parsed: Vec<(PathBuf, Vec<UnifiedMessage>)> = paths
            .into_iter()
            .map(|path| {
                let messages = parse_prime_agent_file(&path);
                (path, messages)
            })
            .collect();
        let accounting: Vec<PrimeFileAccounting> = parsed
            .iter()
            .map(|(path, messages)| analyze_prime_agent_accounting(path, messages))
            .collect();
        let messages: Vec<UnifiedMessage> = parsed
            .into_iter()
            .flat_map(|(_, messages)| messages)
            .collect();
        let messages = reconcile_prime_agent_messages(messages, &accounting);

        let parent_input = messages
            .iter()
            .find(|message| {
                message.dedup_key.as_deref() == Some("prime-agent:response:parent-response")
            })
            .map_or(0, |message| message.tokens.input);
        (
            messages.iter().map(|message| message.tokens.input).sum(),
            parent_input,
        )
    }

    // The parent keeps only its own 100; each of the two 100-token children
    // is counted once from its own transcript.
    assert_eq!(totals(false), (300, 100));
    assert_eq!(totals(true), (300, 100));
}

/// A surviving child response with no completion timestamp cannot prove it
/// is the child a timed attribution describes. Accepting it because it was
/// the only same-sized bucket let an unrelated sibling authorize the
/// subtraction of a pruned child, undercounting billable usage.
#[test]
fn an_untimed_sibling_child_does_not_authorize_a_pruned_child_subtraction() {
    fn totals(reverse: bool) -> (i64, i64) {
        let dir = tempfile::TempDir::new().unwrap();
        let parent_path = dir.path().join("parent.jsonl");
        let sibling_path = dir.path().join("sibling.jsonl");
        // The attributed child transcript is gone; only the attribution
        // records that it spent 50 input tokens inside the 150 aggregate.
        std::fs::write(
            &parent_path,
            r#"{"type":"session","version":3,"id":"parent","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}
{"type":"child_usage_attributed","id":"usage-a","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50},"aggregateUsage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150},"origin":"spawn_task"}
"#,
        )
        .unwrap();
        // An unrelated child of the same parent that happens to have spent
        // the same 50 input tokens, and whose entry carries no timestamp.
        std::fs::write(
            &sibling_path,
            format!(
                r#"{{"type":"session","version":3,"id":"sibling","timestamp":"2026-08-08T00:00:20.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"sibling-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
"#,
                serde_json::to_string(&parent_path.to_string_lossy()).unwrap()
            ),
        )
        .unwrap();

        let mut paths = vec![parent_path, sibling_path];
        if reverse {
            paths.reverse();
        }
        let parsed: Vec<(PathBuf, Vec<UnifiedMessage>)> = paths
            .into_iter()
            .map(|path| {
                let messages = parse_prime_agent_file(&path);
                (path, messages)
            })
            .collect();
        let accounting: Vec<PrimeFileAccounting> = parsed
            .iter()
            .map(|(path, messages)| analyze_prime_agent_accounting(path, messages))
            .collect();
        let messages: Vec<UnifiedMessage> = parsed
            .into_iter()
            .flat_map(|(_, messages)| messages)
            .collect();
        let messages = reconcile_prime_agent_messages(messages, &accounting);

        let parent_input = messages
            .iter()
            .find(|message| {
                message.dedup_key.as_deref() == Some("prime-agent:response:parent-response")
            })
            .map_or(0, |message| message.tokens.input);
        (
            messages.iter().map(|message| message.tokens.input).sum(),
            parent_input,
        )
    }

    // The aggregate parent keeps its full 150 because the child it names was
    // never parsed, and the untimed sibling adds its own 50 on top.
    assert_eq!(totals(false), (200, 150));
    assert_eq!(totals(true), (200, 150));
}

/// Transcripts written before Prime timestamped its entries carry no timing
/// on either side of the pair. Lineage plus usage is then the only identity
/// that exists, so it must still authorize the subtraction rather than
/// double counting every legacy child.
#[test]
fn timestampless_transcripts_still_match_on_lineage_and_usage() {
    fn totals(reverse: bool) -> (i64, i64) {
        let dir = tempfile::TempDir::new().unwrap();
        let parent_path = dir.path().join("parent.jsonl");
        let child_path = dir.path().join("child.jsonl");
        std::fs::write(
            &parent_path,
            r#"{"type":"session","version":3,"id":"parent","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}
{"type":"child_usage_attributed","id":"usage-a","parentId":"parent","targetId":"parent","childUsage":{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50},"aggregateUsage":{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150},"origin":"spawn_task"}
"#,
        )
        .unwrap();
        std::fs::write(
            &child_path,
            format!(
                r#"{{"type":"session","version":3,"id":"child","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
"#,
                serde_json::to_string(&parent_path.to_string_lossy()).unwrap()
            ),
        )
        .unwrap();

        let mut paths = vec![parent_path, child_path];
        if reverse {
            paths.reverse();
        }
        let parsed: Vec<(PathBuf, Vec<UnifiedMessage>)> = paths
            .into_iter()
            .map(|path| {
                let messages = parse_prime_agent_file(&path);
                (path, messages)
            })
            .collect();
        let accounting: Vec<PrimeFileAccounting> = parsed
            .iter()
            .map(|(path, messages)| analyze_prime_agent_accounting(path, messages))
            .collect();
        let messages: Vec<UnifiedMessage> = parsed
            .into_iter()
            .flat_map(|(_, messages)| messages)
            .collect();
        let messages = reconcile_prime_agent_messages(messages, &accounting);

        let parent_input = messages
            .iter()
            .find(|message| {
                message.dedup_key.as_deref() == Some("prime-agent:response:parent-response")
            })
            .map_or(0, |message| message.tokens.input);
        (
            messages.iter().map(|message| message.tokens.input).sum(),
            parent_input,
        )
    }

    assert_eq!(totals(false), (150, 100));
    assert_eq!(totals(true), (150, 100));
}
