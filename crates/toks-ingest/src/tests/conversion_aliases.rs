use super::*;
#[test]
fn test_apply_pricing_if_available_prices_claude_code_gpt_5_3_codex() {
    let pricing = pricing::PricingService::new(HashMap::new(), HashMap::new());

    let mut msg = UnifiedMessage::new(
        "claude",
        "gpt-5.3-codex",
        "openai",
        "session-1",
        1_776_000_000_000,
        TokenBreakdown {
            input: 1_000_000,
            output: 100_000,
            cache_read: 50_000,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    let expected = 1.75 + 1.4 + 0.00875;
    assert!((msg.cost - expected).abs() < 1e-12);
}

#[test]
fn test_apply_pricing_if_available_prices_minimax_m3_bare_id_via_alias() {
    // #935: routers report MiniMax M3 as the bare lowercase id `minimax-m3`,
    // which is not a key in any dataset. When the session record carries no
    // usable provider hint — parsers emit `unknown` for an absent provider
    // and `normalize_provider_hint` drops it — nothing pins the lookup to
    // MiniMax's catalog, so the bare id falls through to model-part/fuzzy
    // matching over every row whose model part is `minimax-m3`.
    //
    // models.dev publishes that model part under dozens of third parties,
    // several of them at 0.0/0.0 (`kenari/minimax-m3` and
    // `nvidia/minimaxai/minimax-m3` both do today). Electing one of those
    // prices real usage at exactly $0 — which is what "pricing missing"
    // in #935 looks like from the user's side, since a row of explicit
    // zeros still counts as "priced" downstream. The alias must pin the
    // canonical first-party `minimax/MiniMax-M3` key instead.
    let mut litellm = HashMap::new();
    // Real first-party rates.
    litellm.insert(
        "minimax/MiniMax-M3".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(3e-7),
            output_cost_per_token: Some(1.2e-6),
            ..Default::default()
        },
    );
    // The hosted reseller row that ships alongside it, at a deliberately
    // far-apart rate so electing it could not be mistaken for the
    // first-party result.
    litellm.insert(
        "fireworks_ai/minimax-m3".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(3e-5),
            output_cost_per_token: Some(1.2e-4),
            ..Default::default()
        },
    );
    // The zero-cost third-party row that the bare id actually elects
    // without the alias.
    let mut models_dev = HashMap::new();
    models_dev.insert(
        "kenari/minimax-m3".to_string(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.0),
            output_cost_per_token: Some(0.0),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new_with_custom_and_models_dev(
        Default::default(),
        litellm,
        HashMap::new(),
        models_dev,
    );

    // Fixture guards: both competing rows must really be in the dataset and
    // resolvable, or this test would pass for the wrong reason.
    let competing_zero = pricing
        .lookup_with_source_and_provider("kenari/minimax-m3", None, None)
        .expect("competing zero-cost models.dev row must be present");
    assert_eq!(competing_zero.matched_key, "kenari/minimax-m3");
    assert_eq!(competing_zero.pricing.input_cost_per_token, Some(0.0));
    let competing_hosted = pricing
        .lookup_with_source_and_provider("minimax-m3", None, Some("fireworks_ai"))
        .expect("competing fireworks_ai row must resolve under its own hint");
    assert_eq!(competing_hosted.matched_key, "fireworks_ai/minimax-m3");

    // The behavior the alias exists to guarantee: the bare id resolves to
    // the canonical first-party key, not to either competitor.
    let resolved = pricing
        .lookup_with_source_and_provider("minimax-m3", None, Some("unknown"))
        .expect("bare `minimax-m3` must resolve");
    assert_eq!(resolved.matched_key, "minimax/MiniMax-M3");
    assert_eq!(resolved.source, "LiteLLM");

    let mut msg = UnifiedMessage::new(
        "ollama",
        "minimax-m3",
        "unknown",
        "session-1",
        1_776_000_000_000,
        TokenBreakdown {
            input: 1_000_000,
            output: 100_000,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    // First-party: 1_000_000 * 3e-7 + 100_000 * 1.2e-6 = 0.42.
    // The zero-cost row would give 0.0; the fireworks row would give 42.0.
    let expected = 1_000_000.0 * 3e-7 + 100_000.0 * 1.2e-6;
    assert!(
        (msg.cost - expected).abs() < 1e-12,
        "expected first-party minimax/MiniMax-M3 cost {expected}, got {}",
        msg.cost
    );
}

#[test]
fn test_apply_pricing_if_available_prices_claude_code_minimax_model() {
    let mut litellm = HashMap::new();
    litellm.insert(
        "minimax/minimax-m2.1".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(0.01),
            output_cost_per_token: Some(0.02),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(litellm, HashMap::new());

    let mut msg = UnifiedMessage::new(
        "claude",
        "MiniMax-M2.1",
        "minimax",
        "session-1",
        1_776_000_000_000,
        TokenBreakdown {
            input: 10,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    assert_eq!(msg.cost, 0.2);
}

#[test]
fn test_apply_pricing_if_available_prices_kimi_k2p6_alias() {
    let mut openrouter = HashMap::new();
    openrouter.insert(
        "moonshotai/kimi-k2.6".into(),
        pricing::ModelPricing {
            input_cost_per_token: Some(9.5e-7),
            output_cost_per_token: Some(0.000004),
            ..Default::default()
        },
    );
    let pricing = pricing::PricingService::new(HashMap::new(), openrouter);

    let mut msg = UnifiedMessage::new(
        "kimi",
        "k2p6",
        "kimi-for-coding",
        "session-1",
        1_776_000_000_000,
        TokenBreakdown {
            input: 1_000_000,
            output: 250_000,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );

    apply_pricing_if_available(&mut msg, Some(&pricing));

    let expected = 1_000_000.0 * 9.5e-7 + 250_000.0 * 0.000004;
    assert!((msg.cost - expected).abs() < 1e-12);
    assert!(msg.cost > 0.0);
}
