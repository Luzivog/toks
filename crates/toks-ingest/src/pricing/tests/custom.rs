use super::*;

#[test]
fn custom_override_wins_over_litellm() {
    let mut custom = HashMap::new();
    custom.insert("gpt-4o".into(), model_pricing(0.000002, 0.000008));
    let mut litellm = HashMap::new();
    litellm.insert("gpt-4o".into(), model_pricing(0.00001, 0.00003));

    let service = custom_service(custom, litellm, HashMap::new());
    let result = service.lookup_with_source("gpt-4o", None).unwrap();

    assert_eq!(result.source, "Custom");
    assert_eq!(result.matched_key, "gpt-4o");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.000002));
}

#[test]
fn custom_override_wins_over_openrouter() {
    let mut custom = HashMap::new();
    custom.insert("grok-code".into(), model_pricing(0.000002, 0.000008));
    let mut openrouter = HashMap::new();
    openrouter.insert("x-ai/grok-code".into(), model_pricing(0.00001, 0.00003));

    let service = custom_service(custom, HashMap::new(), openrouter);
    let result = service.lookup_with_source("grok-code", None).unwrap();

    assert_eq!(result.source, "Custom");
    assert_eq!(result.matched_key, "grok-code");
    assert_eq!(result.pricing.output_cost_per_token, Some(0.000008));
}

#[test]
fn custom_override_respects_force_source() {
    let mut custom = HashMap::new();
    custom.insert("gpt-4o".into(), model_pricing(0.000002, 0.000008));
    let mut litellm = HashMap::new();
    litellm.insert("gpt-4o".into(), model_pricing(0.00001, 0.00003));
    let mut openrouter = HashMap::new();
    openrouter.insert("openai/gpt-4o".into(), model_pricing(0.000003, 0.000012));

    let service = custom_service(custom, litellm, openrouter);

    let litellm_result = service
        .lookup_with_source("gpt-4o", Some("litellm"))
        .unwrap();
    assert_eq!(litellm_result.source, "LiteLLM");
    assert_eq!(litellm_result.pricing.input_cost_per_token, Some(0.00001));

    let openrouter_result = service
        .lookup_with_source("gpt-4o", Some("openrouter"))
        .unwrap();
    assert_eq!(openrouter_result.source, "OpenRouter");
    assert_eq!(
        openrouter_result.pricing.input_cost_per_token,
        Some(0.000003)
    );

    let custom_result = service
        .lookup_with_source("gpt-4o", Some("custom"))
        .unwrap();
    assert_eq!(custom_result.source, "Custom");
    assert_eq!(custom_result.pricing.input_cost_per_token, Some(0.000002));
}

#[test]
fn custom_force_source_does_not_fall_through_on_miss() {
    let mut litellm = HashMap::new();
    litellm.insert("gpt-4o".into(), model_pricing(0.0000025, 0.00001));

    let service = custom_service(HashMap::new(), litellm, HashMap::new());

    assert!(service
        .lookup_with_source("gpt-4o", Some("custom"))
        .is_none());
}

#[test]
fn custom_override_raw_match_wins() {
    let mut custom = HashMap::new();
    custom.insert(
        "accounts/fireworks/routers/kimi-k2p6-turbo".into(),
        model_pricing(0.000002, 0.000008),
    );
    let mut litellm = HashMap::new();
    litellm.insert("kimi-k2.6".into(), model_pricing(0.00000095, 0.000004));

    let service = custom_service(custom, litellm, HashMap::new());
    let result = service
        .lookup_with_source("accounts/fireworks/routers/kimi-k2p6-turbo", None)
        .unwrap();

    assert_eq!(result.source, "Custom");
    assert_eq!(
        result.matched_key,
        "accounts/fireworks/routers/kimi-k2p6-turbo"
    );
    assert_eq!(result.pricing.input_cost_per_token, Some(0.000002));
}

#[test]
fn custom_override_normalized_match_wins() {
    let mut custom = HashMap::new();
    custom.insert("kimi-k2p6".into(), model_pricing(0.00000095, 0.000004));
    let mut litellm = HashMap::new();
    litellm.insert("gpt-4-turbo".into(), model_pricing(0.00001, 0.00003));

    let service = custom_service(custom, litellm, HashMap::new());
    let result = service
        .lookup_with_source("accounts/fireworks/models/kimi-k2p6", None)
        .unwrap();

    assert_eq!(result.source, "Custom");
    assert_eq!(result.matched_key, "kimi-k2p6");
    assert_eq!(result.pricing.output_cost_per_token, Some(0.000004));
}

#[test]
fn custom_override_raw_beats_normalized() {
    let mut custom = HashMap::new();
    custom.insert("kimi-k2p6-turbo".into(), model_pricing(0.000001, 0.000004));
    custom.insert(
        "accounts/fireworks/models/kimi-k2p6-turbo".into(),
        model_pricing(0.000002, 0.000008),
    );

    let service = custom_service(custom, HashMap::new(), HashMap::new());
    let result = service
        .lookup_with_source("accounts/fireworks/models/kimi-k2p6-turbo", None)
        .unwrap();

    assert_eq!(
        result.matched_key,
        "accounts/fireworks/models/kimi-k2p6-turbo"
    );
    assert_eq!(result.pricing.input_cost_per_token, Some(0.000002));
}

#[test]
fn custom_override_skips_fuzzy_chain() {
    let mut custom = HashMap::new();
    custom.insert("kimi-k2p6-turbo".into(), model_pricing(0.000002, 0.000008));

    let service = custom_service(custom, HashMap::new(), HashMap::new());

    assert!(service
        .lookup_with_source("my-kimi-k2p6-turbo", None)
        .is_none());
}

#[test]
fn no_custom_falls_through_to_litellm() {
    let mut litellm = HashMap::new();
    litellm.insert("gpt-4o".into(), model_pricing(0.0000025, 0.00001));

    let service = custom_service(HashMap::new(), litellm, HashMap::new());
    let result = service.lookup_with_source("gpt-4o", None).unwrap();

    assert_eq!(result.source, "LiteLLM");
    assert_eq!(result.pricing.input_cost_per_token, Some(0.0000025));
}

#[test]
fn custom_calculate_cost_uses_override() {
    let mut custom = HashMap::new();
    custom.insert(
        "accounts/fireworks/routers/kimi-k2p6-turbo".into(),
        model_pricing(0.000002, 0.000008),
    );
    let mut litellm = HashMap::new();
    litellm.insert(
        "accounts/fireworks/routers/kimi-k2p6-turbo".into(),
        model_pricing(0.00001, 0.00003),
    );

    let service = custom_service(custom, litellm, HashMap::new());
    let cost = service.calculate_cost(
        "accounts/fireworks/routers/kimi-k2p6-turbo",
        1_000_000,
        100_000,
        0,
        0,
        0,
    );

    let expected = 1_000_000.0 * 0.000002 + 100_000.0 * 0.000008;
    assert!((cost - expected).abs() < 1e-10);
}
