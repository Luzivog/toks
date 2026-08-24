use super::super::*;

#[test]
fn test_provider_hint_empty_and_unknown_treated_as_none() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            ..Default::default()
        },
    );
    litellm.insert(
        "azure_ai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let r_none = lookup.lookup_with_provider("gpt-4", None).unwrap();
    let r_empty = lookup.lookup_with_provider("gpt-4", Some("")).unwrap();
    let r_unknown = lookup
        .lookup_with_provider("gpt-4", Some("unknown"))
        .unwrap();

    assert_eq!(r_none.matched_key, r_empty.matched_key);
    assert_eq!(r_none.matched_key, r_unknown.matched_key);
}

#[test]
fn test_provider_hint_mistralai_matches_mistral_keys() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "mistralai/mistral-large".into(),
        ModelPricing {
            input_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup
        .lookup_with_provider("mistral-large", Some("mistral"))
        .unwrap();
    assert_eq!(result.matched_key, "mistralai/mistral-large");
}

#[test]
fn test_provider_hint_minimax_matches_minimax_keys() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "minimax/minimax-m2.1".into(),
        ModelPricing {
            input_cost_per_token: Some(0.002),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let result = lookup
        .lookup_with_provider("MiniMax-M2.1", Some("minimax"))
        .unwrap();
    assert_eq!(result.matched_key, "minimax/minimax-m2.1");
}

#[test]
fn test_prefixed_model_with_conflicting_provider_uses_provider_aware_path() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            ..Default::default()
        },
    );
    litellm.insert(
        "azure/openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let r_azure = lookup
        .lookup_with_provider("openai/gpt-4", Some("azure"))
        .unwrap();
    assert_eq!(
        r_azure.matched_key, "azure/openai/gpt-4",
        "should prefer azure key when provider_id=azure"
    );

    let r_openai = lookup
        .lookup_with_provider("openai/gpt-4", Some("openai"))
        .unwrap();
    assert_eq!(
        r_openai.matched_key, "openai/gpt-4",
        "should use exact prefixed key when provider_id matches prefix"
    );

    let r_none = lookup.lookup_with_provider("openai/gpt-4", None).unwrap();
    assert_eq!(
        r_none.matched_key, "openai/gpt-4",
        "should use exact prefixed key when no provider hint"
    );
}

#[test]
fn test_prefixed_model_conflicting_provider_falls_back_to_stripped() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            ..Default::default()
        },
    );
    litellm.insert(
        "gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.001),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let r = lookup
        .lookup_with_provider("openai/gpt-4", Some("azure"))
        .unwrap();
    assert_eq!(
        r.matched_key, "gpt-4",
        "with no azure-specific key, should fall back to stripped generic"
    );
}

#[test]
fn test_compound_provider_hint_prefers_reseller_over_prefix() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            ..Default::default()
        },
    );
    litellm.insert(
        "azure/openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());
    let r = lookup
        .lookup_with_provider("openai/gpt-4", Some("azure/openai"))
        .unwrap();
    assert_eq!(
        r.matched_key, "azure/openai/gpt-4",
        "compound hint azure/openai should prefer azure-specific key over openai/ prefix"
    );
}

#[test]
fn test_source_and_provider_normalizes_unknown_hint() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "openai/gpt-4".into(),
        ModelPricing {
            input_cost_per_token: Some(0.01),
            ..Default::default()
        },
    );

    let lookup = PricingLookup::new(litellm, HashMap::new(), HashMap::new());

    let r_unknown = lookup
        .lookup_with_source_and_provider("openai/gpt-4", None, Some("unknown"))
        .unwrap();
    let r_none = lookup
        .lookup_with_source_and_provider("openai/gpt-4", None, None)
        .unwrap();
    assert_eq!(
        r_unknown.matched_key, r_none.matched_key,
        "unknown hint via source_and_provider should behave like None"
    );
}
