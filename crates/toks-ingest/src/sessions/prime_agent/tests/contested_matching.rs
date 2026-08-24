use super::*;

/// Maximum-cardinality matching alone leaves which attribution wins a
/// contested child response up to the order attributions happen to be
/// visited in, which is their random 8-hex id order. The global token total
/// survives that, but the per-model rows do not, and pricing is applied per
/// model after reconciliation -- so the cost of a pruned child lands on the
/// wrong model. The nearer pairing must win regardless of id order.
#[test]
fn the_nearest_attribution_wins_a_contested_child_response() {
    fn per_model_input(reverse: bool, swap_ids: bool) -> HashMap<String, i64> {
        let (id_a, id_b) = if swap_ids {
            ("ffffffff", "00000000")
        } else {
            ("00000000", "ffffffff")
        };
        let dir = tempfile::TempDir::new().unwrap();
        let parent_path = dir.path().join("parent.jsonl");
        let child_path = dir.path().join("child.jsonl");
        // Two parent responses, each persisting a 150 aggregate that is 100
        // of its own plus one 50-token child. Only the second parent's child
        // transcript survives; the first parent's child was pruned.
        std::fs::write(
            &parent_path,
            format!(
                r#"{{"type":"session","version":3,"id":"parent","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}}
{{"type":"message","id":"parent-a","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{{"role":"assistant","provider":"anthropic","model":"model-a","responseId":"parent-response-a","usage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}}}}
{{"type":"child_usage_attributed","id":"{id_a}","parentId":"parent-a","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent-a","childUsage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}},"aggregateUsage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}},"origin":"spawn_task"}}
{{"type":"message","id":"parent-b","parentId":"{id_a}","timestamp":"2026-08-08T00:00:01.500Z","message":{{"role":"assistant","provider":"anthropic","model":"model-b","responseId":"parent-response-b","usage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}}}}
{{"type":"child_usage_attributed","id":"{id_b}","parentId":"parent-b","timestamp":"2026-08-08T00:00:02.002Z","targetId":"parent-b","childUsage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}},"aggregateUsage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}},"origin":"spawn_task"}}
"#
            ),
        )
        .unwrap();
        // The surviving child completed in the same millisecond as the
        // second parent's attribution, and two milliseconds from the first
        // parent's -- inside the tolerance window for both.
        std::fs::write(
            &child_path,
            format!(
                r#"{{"type":"session","version":3,"id":"child","timestamp":"2026-08-08T00:00:01.600Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.002Z","message":{{"role":"assistant","provider":"anthropic","model":"child-model","responseId":"child-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
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

        let mut per_model: HashMap<String, i64> = HashMap::new();
        for message in &messages {
            *per_model.entry(message.model_id.clone()).or_default() += message.tokens.input;
        }
        per_model
    }

    for reverse in [false, true] {
        for swap_ids in [false, true] {
            let per_model = per_model_input(reverse, swap_ids);
            assert_eq!(
                per_model.get("model-a").copied(),
                Some(150),
                "the parent whose child was pruned keeps its aggregate \
                 (reverse={reverse}, swap_ids={swap_ids})"
            );
            assert_eq!(
                per_model.get("model-b").copied(),
                Some(100),
                "the parent whose child survived keeps only its own usage \
                 (reverse={reverse}, swap_ids={swap_ids})"
            );
            assert_eq!(per_model.get("child-model").copied(), Some(50));
            assert_eq!(per_model.values().sum::<i64>(), 300);
        }
    }
}

/// Two fork copies that name each other as fork parent describe one fork
/// history, so their copies of one attribution must collapse. Resolving each
/// copy to itself instead makes the pair look like two independent
/// attributions, and the unavailable child's delta is restored once per copy.
#[test]
fn a_fork_parent_loop_collapses_onto_one_lineage() {
    fn totals(reverse: bool, with_child: bool) -> (i64, i64) {
        let dir = tempfile::TempDir::new().unwrap();
        let first_path = dir.path().join("fork-a.jsonl");
        let second_path = dir.path().join("fork-b.jsonl");
        let child_path = dir.path().join("child.jsonl");
        // Each copy names the other as its fork parent, and both carry the
        // same response, the same attribution id, and the same 150 aggregate
        // that is 100 of their own plus one 50-token child.
        for (path, fork_parent) in [(&first_path, &second_path), (&second_path, &first_path)] {
            std::fs::write(
                path,
                format!(
                    r#"{{"type":"session","version":3,"id":"fork","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":0}}
{{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"shared-parent","usage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}}}}
{{"type":"child_usage_attributed","id":"aaaa1111","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}},"aggregateUsage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}},"origin":"spawn_task"}}
"#,
                    serde_json::to_string(&fork_parent.to_string_lossy()).unwrap()
                ),
            )
            .unwrap();
        }
        if with_child {
            std::fs::write(
                &child_path,
                format!(
                    r#"{{"type":"session","version":3,"id":"child","timestamp":"2026-08-08T00:00:01.500Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.000Z","message":{{"role":"assistant","provider":"anthropic","model":"child-model","responseId":"child-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
"#,
                    serde_json::to_string(&first_path.to_string_lossy()).unwrap()
                ),
            )
            .unwrap();
        }

        let mut paths = vec![first_path, second_path];
        if with_child {
            paths.push(child_path);
        }
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
                message.dedup_key.as_deref() == Some("prime-agent:response:shared-parent")
            })
            .map_or(0, |message| message.tokens.input);
        (
            messages.iter().map(|message| message.tokens.input).sum(),
            parent_input,
        )
    }

    for reverse in [false, true] {
        // The child was never parsed, so the one aggregate is kept whole and
        // counted once rather than once per fork copy.
        assert_eq!(totals(reverse, false), (150, 150), "reverse={reverse}");
        // The child transcript is available, so the collapsed parent keeps
        // only its own 100 and the child is counted once from its own file.
        assert_eq!(totals(reverse, true), (150, 100), "reverse={reverse}");
    }
}

/// The partial case: three parent responses each claim a 50-token child
/// inside one timestamp window, but only two of those child transcripts
/// survive. Maximum cardinality fixes that two attributions are matched
/// without saying which two, so the surviving transcripts must go to the
/// attributions they are nearest to, not to whichever the scan reaches first.
#[test]
fn partial_equal_usage_matches_keep_each_attributions_identity() {
    fn per_model_input(reverse: bool, descending_ids: bool) -> HashMap<String, i64> {
        let mut ids = ["00000000", "88888888", "ffffffff"];
        if descending_ids {
            ids.reverse();
        }
        let dir = tempfile::TempDir::new().unwrap();
        let parent_path = dir.path().join("parent.jsonl");
        let mut parent = r#"{"type":"session","version":3,"id":"parent","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
"#
        .to_string();
        // model-a's child was pruned; model-b's and model-c's survived, each
        // completing in the same millisecond as its own attribution.
        for (model, id, millis) in [
            ("model-a", ids[0], "000"),
            ("model-b", ids[1], "003"),
            ("model-c", ids[2], "006"),
        ] {
            parent.push_str(&format!(
                r#"{{"type":"message","id":"parent-{model}","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{{"role":"assistant","provider":"anthropic","model":"{model}","responseId":"response-{model}","usage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}}}}}}
{{"type":"child_usage_attributed","id":"{id}","parentId":"parent-{model}","timestamp":"2026-08-08T00:00:02.{millis}Z","targetId":"parent-{model}","childUsage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}},"aggregateUsage":{{"input":150,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":150}},"origin":"spawn_task"}}
"#
            ));
        }
        std::fs::write(&parent_path, parent).unwrap();

        let mut paths = vec![parent_path.clone()];
        for (name, millis) in [("child-b", "003"), ("child-c", "006")] {
            let child_path = dir.path().join(format!("{name}.jsonl"));
            std::fs::write(
                &child_path,
                format!(
                    r#"{{"type":"session","version":3,"id":"{name}","timestamp":"2026-08-08T00:00:01.500Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.{millis}Z","message":{{"role":"assistant","provider":"anthropic","model":"child-model","responseId":"{name}-response","usage":{{"input":50,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":50}}}}}}
"#,
                    serde_json::to_string(&parent_path.to_string_lossy()).unwrap()
                ),
            )
            .unwrap();
            paths.push(child_path);
        }
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

        let mut per_model: HashMap<String, i64> = HashMap::new();
        for message in &messages {
            *per_model.entry(message.model_id.clone()).or_default() += message.tokens.input;
        }
        per_model
    }

    for reverse in [false, true] {
        for descending_ids in [false, true] {
            let per_model = per_model_input(reverse, descending_ids);
            let context = format!("reverse={reverse}, descending_ids={descending_ids}");
            assert_eq!(
                per_model.get("model-a").copied(),
                Some(150),
                "the pruned child's aggregate is retained ({context})"
            );
            assert_eq!(
                per_model.get("model-b").copied(),
                Some(100),
                "model-b's own child authorizes its subtraction ({context})"
            );
            assert_eq!(
                per_model.get("model-c").copied(),
                Some(100),
                "model-c's own child authorizes its subtraction ({context})"
            );
            assert_eq!(per_model.get("child-model").copied(), Some(100));
            assert_eq!(per_model.values().sum::<i64>(), 450);
        }
    }
}
