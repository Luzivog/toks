use super::*;
#[test]
fn graph_pricing_policy_excludes_unpriced_only_from_submission() {
    let message = UnifiedMessage::new(
        "opencode",
        "genuinely-unpriced-model",
        "unknown-provider",
        "unpriced",
        1_736_510_400_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );
    // Populated but not covering this model: an empty service would instead
    // trip the "no pricing dataset loaded" guard, which is a different case.
    let pricing = pricing::PricingService::new(unrelated_litellm_dataset(), HashMap::new());

    let report = build_graph_from_messages(
        vec![message.clone()],
        Some(&pricing),
        GraphPricingRequirement::Lenient,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("reporting graphs should retain unpriced usage");
    let submission = build_graph_from_messages(
        vec![message],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("submission graphs should exclude unpriced usage");

    assert_eq!(report.summary.total_tokens, 1);
    assert_eq!(submission.summary.total_tokens, 0);
    assert_eq!(submission.unpriced_submission_exclusions.len(), 1);
    assert_eq!(
        submission.unpriced_submission_exclusions[0].model_id,
        "genuinely-unpriced-model"
    );
}

/// A dataset that loaded successfully but prices an unrelated model.
///
/// Tests asserting "this model is unpriced" must use this rather than an
/// empty service: an empty service means *no dataset loaded*, which is a
/// separate, fatal condition on the submission path.
pub(super) fn unrelated_litellm_dataset() -> HashMap<String, pricing::ModelPricing> {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4o".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(1e-6),
            output_cost_per_token: Some(2e-6),
            ..Default::default()
        },
    );
    litellm
}

#[test]
fn submission_without_any_pricing_data_still_fails() {
    let message = UnifiedMessage::new(
        "opencode",
        "gpt-4o",
        "openai",
        "priced-if-data-loaded",
        1_736_510_400_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );

    // `None` is unreachable from `generate_submission_graph`, which always
    // passes `Some(..)` because `PricingService::get_or_init` degrades every
    // failed source to an empty map rather than erroring. The reachable
    // shape of "no pricing dataset loaded" is a populated-with-nothing
    // service, so both must fail identically.
    let empty = pricing::PricingService::new(HashMap::new(), HashMap::new());
    for (label, pricing) in [
        ("no service at all", None),
        ("a service with no dataset", Some(&empty)),
    ] {
        let Err(error) = build_graph_from_messages(
            vec![message.clone()],
            pricing,
            GraphPricingRequirement::Submission,
            std::time::Instant::now(),
            &crate::bucket_tz::BucketTimezone::Local,
        ) else {
            panic!("submission must fail with {label}");
        };

        assert_eq!(error, "pricing data is unavailable for submission");
    }
}

/// A missing pricing dataset only matters if something needed pricing.
/// Provider-reported costs are authoritative, so a batch made entirely of
/// them must still submit during a total upstream outage.
#[test]
fn submission_without_pricing_data_still_accepts_provider_reported_usage() {
    let mut message = UnifiedMessage::new(
        "opencode",
        "some-model",
        "anthropic",
        "provider-reported",
        1_736_510_400_000,
        TokenBreakdown {
            input: 1_000,
            output: 500,
            ..Default::default()
        },
        0.05,
    );
    message.mark_provider_reported_cost();
    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());

    let graph = build_graph_from_messages(
        vec![message],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("authoritative costs must not need a pricing dataset");

    assert_eq!(graph.summary.total_tokens, 1_500);
    assert!(graph.unpriced_submission_exclusions.is_empty());
}

#[test]
fn submission_excludes_unpriced_generic_gemini_default_but_keeps_priceable_usage() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4o".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(1e-6),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());
    let mut generic = UnifiedMessage::new(
        "antigravity-cli",
        "gemini-default",
        "google",
        "generic",
        1_736_510_400_000,
        TokenBreakdown {
            input: 7,
            cache_read: 11,
            ..Default::default()
        },
        0.0,
    );
    generic.message_count = 7;
    let concrete = UnifiedMessage::new(
        "synthetic",
        "gpt-4o",
        "openai",
        "concrete",
        1_736_510_400_000,
        TokenBreakdown {
            input: 13,
            ..Default::default()
        },
        0.0,
    );

    let graph = build_graph_from_messages(
        vec![generic, concrete],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("generic routing label must not block fully priced submission usage");

    assert_eq!(graph.summary.total_tokens, 13);
    assert_eq!(graph.contributions[0].clients.len(), 1);
    assert_eq!(graph.contributions[0].clients[0].model_id, "gpt-4o");
    assert_eq!(graph.unpriced_submission_exclusions.len(), 1);
    assert_eq!(
        graph.unpriced_submission_exclusions[0],
        UnpricedSubmissionExclusion {
            provider_id: "google".to_string(),
            model_id: "gemini-default".to_string(),
            message_count: 7,
            total_tokens: 18,
            reason: ROUTING_LABEL_UNPRICED_REASON,
        }
    );
}

#[test]
fn submission_excludes_unpriced_auto_routing_label() {
    // `auto` is the unknown-model label Kiro emits and the default-model
    // label Cursor/Copilot record in usage rows. A models.dev `morph/auto`
    // paid row exists, so before the resolver refused routing labels the
    // bare label resolved to it and slipped through submission at morph's
    // rates (#1062). The fixture quotes a cache-read rate precisely so the
    // pre-fix fallback covers all three populated buckets (7 input, 11
    // cache-read, 0 output) and submits the row — without it, the row
    // would fail coverage on the missing cache rate and the test would
    // pass even with the bug. The label must instead be excluded with the
    // routing-label reason.
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "morph/auto".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(8.5e-7),
            output_cost_per_token: Some(1.55e-6),
            cache_read_input_token_cost: Some(1.6e-7),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new_with_custom_and_models_dev(
        pricing::custom::CustomPricing::default(),
        HashMap::new(),
        HashMap::new(),
        models_dev,
    );
    let mut auto = UnifiedMessage::new(
        "kiro",
        "auto",
        "amazon-bedrock",
        "generic",
        1_736_510_400_000,
        TokenBreakdown {
            input: 7,
            cache_read: 11,
            ..Default::default()
        },
        0.0,
    );
    auto.message_count = 7;

    let graph = build_graph_from_messages(
        vec![auto],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("routing label must not abort submission");

    assert_eq!(graph.summary.total_tokens, 0);
    assert_eq!(graph.unpriced_submission_exclusions.len(), 1);
    assert_eq!(
        graph.unpriced_submission_exclusions[0],
        UnpricedSubmissionExclusion {
            provider_id: "amazon-bedrock".to_string(),
            model_id: "auto".to_string(),
            message_count: 7,
            total_tokens: 18,
            reason: ROUTING_LABEL_UNPRICED_REASON,
        }
    );
}

#[test]
fn test_submit_default_graph_includes_antigravity_cache_rows() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    // Resolved rather than hardcoded: under an explicit home the config
    // root is `~/.config/toks` on Unix and
    // `%HOME%\AppData\Roaming\toks` on Windows, so the Unix spelling
    // put the fixture outside the tree the scan walks and the graph came
    // back empty.
    let sessions_dir = std::path::PathBuf::from(
        ClientId::Antigravity
            .data()
            .resolve_path_with_env_strategy(&temp_dir.path().to_string_lossy(), false),
    );
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::write(
            sessions_dir.join("ag-submit.jsonl"),
            r#"{"type":"usage","sessionId":"ag-submit","modelId":"model_placeholder_m84","timestamp":1711200000000,"input":12,"output":4,"cacheRead":2,"cacheWrite":0,"reasoning":1,"responseId":"resp-ag"}
"#,
        )
        .unwrap();

    let mut clients: Vec<String> = ClientId::iter()
        .filter(|client| client.submit_default())
        .map(|client| client.as_str().to_string())
        .collect();
    clients.push("synthetic".to_string());

    let rt = tokio::runtime::Runtime::new().unwrap();
    let graph = rt
        .block_on(generate_graph_with_loaded_pricing(
            ReportOptions {
                home_dir: Some(temp_dir.path().to_string_lossy().to_string()),
                use_env_roots: false,
                clients: Some(clients),
                since: None,
                until: None,
                year: None,
                group_by: GroupBy::default(),
                scanner_settings: scanner::ScannerSettings::default(),
            },
            None,
            GraphPricingRequirement::Lenient,
        ))
        .unwrap();

    assert_eq!(graph.summary.clients, vec!["antigravity"]);
    assert_eq!(graph.summary.models, vec!["gemini-3-flash-preview"]);
    assert_eq!(graph.summary.total_tokens, 19);
    assert_eq!(graph.contributions.len(), 1);
    assert_eq!(graph.contributions[0].clients[0].client, "antigravity");
    assert_eq!(
        graph.contributions[0].clients[0].model_id,
        "gemini-3-flash-preview"
    );
}

#[test]
fn test_filter_messages_preserves_pi_9router_when_no_duplicate() {
    let messages = vec![
        UnifiedMessage::new(
            "pi",
            "deepseek_v4_flash_free",
            "9router",
            "session-1",
            1783412353188,
            TokenBreakdown::default(),
            0.0,
        ),
        UnifiedMessage::new(
            "9router",
            "deepseek-ai/deepseek-v4-flash",
            "nvidia",
            "session-2",
            1783412353188,
            TokenBreakdown {
                input: 100,
                output: 50,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            0.05,
        ),
    ];
    // Without verified cross-source dedup, both messages are preserved.
    let filtered = filter_messages_for_report(messages, &ReportOptions::default());
    assert_eq!(filtered.len(), 2);
}
