use super::graphs_submission::unrelated_litellm_dataset;
use super::*;
#[test]
fn whitespace_padded_routing_label_is_classified_the_same_by_resolver_and_reason() {
    // `lookup::is_routing_label` trims before comparing, so the resolver
    // refuses to price ` auto `. The exclusion reason has to agree, or the
    // row is reported as having no model-to-price mapping while the reason
    // it is unpriced is that it names a router. Both paths now read the
    // same list, so a label added to `lookup::ROUTING_LABELS` cannot drift
    // out of the reason.
    assert_eq!(
        crate::pricing::lookup::is_routing_label(" auto "),
        is_generic_routing_label("amazon-bedrock", " auto "),
        "resolver and exclusion reason must classify a padded routing label alike"
    );

    // The models.dev `morph/auto` row is fully priced, so if the resolver
    // did not refuse the padded label the row would submit at Morph rates
    // (#1062) instead of reaching the exclusion path at all.
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
    let mut padded = UnifiedMessage::new(
        "kiro",
        " auto ",
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
    padded.message_count = 7;

    let graph = build_graph_from_messages(
        vec![padded],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("routing label must not abort submission");

    assert_eq!(graph.summary.total_tokens, 0);
    assert_eq!(graph.unpriced_submission_exclusions.len(), 1);
    assert_eq!(
        graph.unpriced_submission_exclusions[0].reason, ROUTING_LABEL_UNPRICED_REASON,
        "padded routing label must report the routing-label reason"
    );
}

#[test]
fn custom_priced_routing_label_reports_incomplete_pricing_not_missing_mapping() {
    // A `custom-pricing.json` entry for a routing label is the user
    // stating what their router actually costs them — the escape hatch
    // `ROUTING_LABELS` documents. Telling that user the label "has no
    // authoritative model-to-price mapping" contradicts the mapping they
    // just wrote. Here the custom entry quotes an input rate but no cache
    // rate, so the row still fails coverage; the reason must name the gap
    // that is actually fixable (the missing cache-read rate), not deny the
    // mapping exists.
    let mut custom_models = HashMap::new();
    custom_models.insert(
        "auto".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(3e-6),
            output_cost_per_token: Some(1.5e-5),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new_with_custom(
        pricing::custom::CustomPricing::from_models(custom_models),
        HashMap::new(),
        HashMap::new(),
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

    assert_eq!(graph.unpriced_submission_exclusions.len(), 1);
    assert_eq!(
        graph.unpriced_submission_exclusions[0],
        UnpricedSubmissionExclusion {
            provider_id: "amazon-bedrock".to_string(),
            model_id: "auto".to_string(),
            message_count: 7,
            total_tokens: 18,
            reason: INCOMPLETE_MODEL_PRICING_REASON,
        }
    );
}

#[test]
fn submission_excludes_unpriced_concrete_models() {
    let concrete = UnifiedMessage::new(
        "synthetic",
        "gemini-3.5-pro",
        "google",
        "concrete",
        1_736_510_400_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );
    // Populated but not covering this model — see `unrelated_litellm_dataset`.
    let pricing = pricing::PricingService::new(unrelated_litellm_dataset(), HashMap::new());

    let graph = build_graph_from_messages(
        vec![concrete],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("one unpriced model must not block the submission");

    assert_eq!(graph.summary.total_tokens, 0);
    assert!(graph.contributions.is_empty());
    assert_eq!(
        graph.unpriced_submission_exclusions,
        vec![UnpricedSubmissionExclusion {
            provider_id: "google".to_string(),
            model_id: "gemini-3.5-pro".to_string(),
            message_count: 1,
            total_tokens: 1,
            reason: MISSING_MODEL_PRICING_REASON,
        }]
    );
}

#[test]
fn submission_excludes_usage_with_an_unpriced_cache_write_bucket() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-5.5".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(5e-6),
            output_cost_per_token: Some(30e-6),
            cache_read_input_token_cost: Some(0.5e-6),
            ..Default::default()
        },
    );
    litellm.insert(
        "gpt-4o".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(1e-6),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());
    let incomplete = UnifiedMessage::new(
        "hermes",
        "gpt-5.5",
        "custom",
        "incomplete",
        1_736_510_400_000,
        TokenBreakdown {
            input: 10,
            cache_read: 20,
            cache_write: 30,
            ..Default::default()
        },
        0.0,
    );
    let covered = UnifiedMessage::new(
        "synthetic",
        "gpt-4o",
        "openai",
        "covered",
        1_736_510_400_000,
        TokenBreakdown {
            input: 40,
            ..Default::default()
        },
        0.0,
    );

    let graph = build_graph_from_messages(
        vec![incomplete, covered],
        Some(&pricing),
        GraphPricingRequirement::Submission,
        std::time::Instant::now(),
        &crate::bucket_tz::BucketTimezone::Local,
    )
    .expect("an incomplete cache rate must not block covered usage");

    assert_eq!(graph.summary.total_tokens, 40);
    assert_eq!(graph.contributions[0].clients[0].model_id, "gpt-4o");
    assert_eq!(
        graph.unpriced_submission_exclusions,
        vec![UnpricedSubmissionExclusion {
            provider_id: "custom".to_string(),
            model_id: "gpt-5.5".to_string(),
            message_count: 1,
            total_tokens: 60,
            reason: INCOMPLETE_MODEL_PRICING_REASON,
        }]
    );
}
