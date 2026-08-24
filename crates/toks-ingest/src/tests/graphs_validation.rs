use super::*;
#[test]
fn strict_pricing_validation_accepts_covered_and_provider_reported_usage() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "covered-model".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());
    let covered = UnifiedMessage::new(
        "synthetic",
        "covered-model",
        "openai",
        "covered",
        1_733_011_200_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );
    let mut reported = UnifiedMessage::new(
        "synthetic",
        "unlisted-model",
        "provider",
        "reported",
        1_733_011_200_000,
        TokenBreakdown {
            output: 1,
            ..Default::default()
        },
        0.0,
    );
    reported.mark_provider_reported_cost();

    assert!(validate_priced_messages(&[covered, reported], Some(&pricing)).is_ok());
}

#[test]
fn strict_pricing_validation_rejects_unpriced_token_usage() {
    let message = UnifiedMessage::new(
        "synthetic",
        "unlisted-model",
        "provider",
        "unpriced",
        1_733_011_200_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );
    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());

    let error = validate_priced_messages(&[message], Some(&pricing)).unwrap_err();
    assert!(error.contains("provider/unlisted-model"));
}

// Regression: #1013. The message used to repeat one entry per affected
// message, so a real submission produced a ~290KB error that scrolled the
// actionable model ids off screen.
#[test]
fn strict_pricing_validation_error_deduplicates_models_with_counts() {
    let unpriced = |model: &str, session: &str| {
        UnifiedMessage::new(
            "synthetic",
            model,
            "provider",
            session,
            1_733_011_200_000,
            TokenBreakdown {
                input: 1,
                ..Default::default()
            },
            0.0,
        )
    };
    let messages = vec![
        unpriced("repeated-model", "a"),
        unpriced("repeated-model", "b"),
        unpriced("repeated-model", "c"),
        unpriced("single-model", "d"),
    ];
    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());

    let error = validate_priced_messages(&messages, Some(&pricing)).unwrap_err();

    assert_eq!(error.matches("provider/repeated-model").count(), 1);
    assert_eq!(error.matches("provider/single-model").count(), 1);
    assert!(
        error.contains("provider/repeated-model (x3)"),
        "repeated ids must carry an occurrence count: {error}"
    );
    assert!(
        !error.contains("provider/single-model (x"),
        "single occurrences must not be annotated: {error}"
    );
    assert!(
        error.find("provider/repeated-model") < error.find("provider/single-model"),
        "ids must keep first-seen order: {error}"
    );
}

#[test]
fn strict_pricing_validation_accepts_bundled_pricing() {
    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());
    let message = UnifiedMessage::new(
        "cursor",
        "composer-2.5",
        "cursor",
        "bundled",
        1_733_011_200_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );

    assert!(validate_priced_messages(&[message], Some(&pricing)).is_ok());
}

#[test]
fn strict_pricing_validation_ignores_filtered_out_unpriced_usage() {
    let mut old = UnifiedMessage::new(
        "synthetic",
        "unlisted-model",
        "provider",
        "old",
        1_733_011_200_000,
        TokenBreakdown {
            input: 1,
            ..Default::default()
        },
        0.0,
    );
    old.date = "2020-01-01".to_string();
    let filtered = filter_messages_for_report(
        vec![old],
        &ReportOptions {
            since: Some("2021-01-01".to_string()),
            ..Default::default()
        },
    );

    assert!(validate_priced_messages(
        &filtered,
        Some(&pricing::PricingService::new(
            HashMap::new(),
            HashMap::new()
        ))
    )
    .is_ok());
}

#[test]
fn strict_pricing_validation_requires_each_populated_bucket_to_have_a_base_rate() {
    let mut custom = HashMap::new();
    custom.insert(
        "input-only".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    custom.insert(
        "output-only".to_string(),
        pricing::ModelPricing {
            output_cost_per_token: Some(1e-6),
            ..Default::default()
        },
    );
    custom.insert(
        "tier-only".to_string(),
        pricing::ModelPricing {
            input_cost_per_token_above_272k_tokens: Some(1e-6),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new_with_custom(
        pricing::custom::CustomPricing::from_models(custom),
        HashMap::new(),
        HashMap::new(),
    );
    let usage = |model, input, output, reasoning, cache_read, cache_write| {
        UnifiedMessage::new(
            "synthetic",
            model,
            "provider",
            model,
            1_733_011_200_000,
            TokenBreakdown {
                input,
                output,
                reasoning,
                cache_read,
                cache_write,
            },
            0.0,
        )
    };

    assert!(
        validate_priced_messages(&[usage("input-only", 1, 0, 0, 0, 0)], Some(&pricing)).is_ok()
    );
    assert!(
        validate_priced_messages(&[usage("input-only", 0, 1, 0, 0, 0)], Some(&pricing)).is_err()
    );
    assert!(
        validate_priced_messages(&[usage("output-only", 0, 1, 1, 0, 0)], Some(&pricing)).is_ok()
    );
    assert!(
        validate_priced_messages(&[usage("output-only", 1, 0, 0, 0, 0)], Some(&pricing)).is_err()
    );
    assert!(
        validate_priced_messages(&[usage("output-only", 0, 0, 0, 1, 0)], Some(&pricing)).is_err()
    );
    assert!(
        validate_priced_messages(&[usage("output-only", 0, 0, 0, 0, 1)], Some(&pricing)).is_err()
    );
    assert!(
        validate_priced_messages(&[usage("tier-only", 300_000, 0, 0, 0, 0)], Some(&pricing))
            .is_err()
    );
}
