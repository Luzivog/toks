use super::*;

#[test]
fn test_provider_tags_normalize_known_aliases() {
    let cases = [
        ("openai-codex", vec!["openai"]),
        ("gemini", vec!["google"]),
        ("vertex", vec!["anthropic"]),
        ("azure", vec!["azure_ai"]),
        ("fireworks", vec!["fireworks_ai"]),
        ("MiniMax", vec!["minimax"]),
        ("openrouter/google", vec!["openrouter", "google"]),
        ("bedrock/anthropic", vec!["bedrock", "anthropic"]),
    ];

    for (raw, expected) in cases {
        assert_eq!(provider_tags(raw), expected);
    }
}

#[test]
fn test_canonical_provider_returns_first_canonical_tag() {
    assert_eq!(canonical_provider("openai-codex"), Some("openai".into()));
    assert_eq!(
        canonical_provider("openrouter/google"),
        Some("openrouter".into())
    );
    assert_eq!(canonical_provider("<synthetic>"), None);
    assert_eq!(canonical_provider("unknown"), None);
}

#[test]
fn test_key_provider_tags_extract_nested_provider_segments() {
    assert_eq!(
        key_provider_tags("openrouter/google/gemini-3-pro-preview"),
        vec!["openrouter", "google"]
    );
    assert_eq!(
        key_provider_tags("bedrock/anthropic.claude-sonnet-4"),
        vec!["bedrock", "anthropic"]
    );
}

#[test]
fn test_matches_provider_hint_for_known_aliases_and_nested_keys() {
    assert!(matches_provider_hint(
        "openai/gpt-5.2-preview",
        Some("openai-codex")
    ));
    assert!(matches_provider_hint(
        "openrouter/google/gemini-3-pro-preview",
        Some("google")
    ));
    assert!(matches_provider_hint("azure/openai/gpt-4", Some("azure")));
    assert!(matches_provider_hint(
        "fireworks_ai/deepseek-v3-0324",
        Some("fireworks")
    ));
    assert!(!matches_provider_hint("openai/gpt-4", Some("anthropic")));
}

#[test]
fn fable_models_map_to_anthropic() {
    // Fable is a Claude model family; the bare, claude-prefixed, and [1m]
    // context-variant forms must all attribute to Anthropic.
    assert_eq!(inferred_provider_from_model("fable-5"), Some("anthropic"));
    assert_eq!(
        inferred_provider_from_model("claude-fable-5"),
        Some("anthropic")
    );
    assert_eq!(
        inferred_provider_from_model("claude-fable-5[1m]"),
        Some("anthropic")
    );
}

#[test]
fn test_inferred_provider_from_model() {
    assert_eq!(
        inferred_provider_from_model("claude-sonnet-4"),
        Some("anthropic")
    );
    assert_eq!(inferred_provider_from_model("gpt-5.2"), Some("openai"));
    assert_eq!(inferred_provider_from_model("gpt-5.5"), Some("openai"));
    assert_eq!(
        inferred_provider_from_model("gemini-2.5-pro"),
        Some("google")
    );
    assert_eq!(
        inferred_provider_from_model("grok-code-fast-1"),
        Some("xai")
    );
    assert_eq!(
        inferred_provider_from_model("deepseek-v3"),
        Some("deepseek")
    );
    assert_eq!(
        inferred_provider_from_model("MiniMax-M2.1"),
        Some("minimax")
    );
    assert_eq!(
        inferred_provider_from_model("mixtral-8x7b"),
        Some("mistral")
    );
    assert_eq!(
        inferred_provider_from_model("mistral-large"),
        Some("mistral")
    );
    assert_eq!(inferred_provider_from_model("llama-3"), Some("meta"));
    assert_eq!(inferred_provider_from_model("qwen3-coder"), Some("qwen"));
    assert_eq!(inferred_provider_from_model("unknown-model"), None);
}

#[test]
fn test_inferred_provider_bare_kimi_k_series_ids() {
    // Kimi's coding-plan catalog serves these with no `kimi` prefix at all.
    assert_eq!(inferred_provider_from_model("k3"), Some("moonshotai"));
    assert_eq!(inferred_provider_from_model("k3-256k"), Some("moonshotai"));
    assert_eq!(inferred_provider_from_model("K3"), Some("moonshotai"));
    assert_eq!(inferred_provider_from_model("k2"), Some("moonshotai"));
    // Already-prefixed forms keep matching via the `kimi` substring check.
    assert_eq!(
        inferred_provider_from_model("kimi-k2.5-thinking"),
        Some("moonshotai")
    );
    // A `k2`/`k3` substring that isn't a delimited token must not match.
    assert_eq!(inferred_provider_from_model("flock3"), None);
    assert_eq!(inferred_provider_from_model("network2"), None);
}

#[test]
fn test_inferred_provider_ignores_ollama_route_prefix() {
    assert_eq!(inferred_provider_from_model("ollama/orca-mini"), None);
    assert_eq!(
        inferred_provider_from_model("ollama/qwen3-coder"),
        Some("qwen")
    );
    assert_eq!(
        inferred_provider_from_model("ollama/llama-3.3"),
        Some("meta")
    );
}

#[test]
fn test_inferred_provider_fugu_maps_to_sakana() {
    assert_eq!(inferred_provider_from_model("fugu"), Some("sakana"));
    assert_eq!(inferred_provider_from_model("fugu-ultra"), Some("sakana"));
    assert_eq!(inferred_provider_from_model("Fugu"), Some("sakana"));
    assert_eq!(inferred_provider_from_model("FUGU-ULTRA"), Some("sakana"));
}

#[test]
fn test_provider_tags_preserves_sakana() {
    assert_eq!(provider_tags("sakana"), vec!["sakana"]);
}

#[test]
fn test_inferred_provider_no_false_positives() {
    assert_eq!(inferred_provider_from_model("protocol1-fast"), None);
    assert_eq!(inferred_provider_from_model("proto3-server"), None);
    assert_eq!(inferred_provider_from_model("co4pilot-v2"), None);
    assert_eq!(inferred_provider_from_model("metadata-model"), None);
    assert_eq!(inferred_provider_from_model("metamorphic-v1"), None);
}

/// The families below are matched with plain `contains`, not
/// `contains_delimited`, and that asymmetry is load-bearing rather than an
/// oversight. Vendors append version digits directly to the family token
/// (`qwen3`, `mistral4`) and embed it mid-word (`chatgpt-4o-latest`,
/// `codellama`), all of which a delimited match rejects.
///
/// Switching these to delimited matching drops the provider on 536 model ids
/// in the bundled models.dev/litellm/openrouter catalogs. `contains_delimited`
/// stays reserved for short tokens that collide inside ordinary words -- see
/// `test_inferred_provider_no_false_positives`.
#[test]
fn test_inferred_provider_matches_version_suffixed_and_embedded_families() {
    for model in [
        "qwen3-coder",
        "qwen3.7-plus",
        "qwen2-5-14b-instruct",
        "qwen3-235b-a22b-instruct-2507",
    ] {
        assert_eq!(inferred_provider_from_model(model), Some("qwen"), "{model}");
    }

    for model in ["chatgpt-4o-latest", "chatgpt-image-latest"] {
        assert_eq!(
            inferred_provider_from_model(model),
            Some("openai"),
            "{model}"
        );
    }

    assert_eq!(
        inferred_provider_from_model("mistral4-119b"),
        Some("mistral")
    );
    assert_eq!(
        inferred_provider_from_model("CodeLlama-34b-Instruct-hf"),
        Some("meta")
    );
}

#[test]
fn test_inferred_provider_boundary_matches() {
    assert_eq!(inferred_provider_from_model("o1-preview"), Some("openai"));
    assert_eq!(inferred_provider_from_model("o3-mini"), Some("openai"));
    assert_eq!(inferred_provider_from_model("o4-mini"), Some("openai"));
    assert_eq!(inferred_provider_from_model("meta-llama-3"), Some("meta"));
}

#[test]
fn test_provider_tags_mistral_alias() {
    assert_eq!(provider_tags("mistral"), vec!["mistralai"]);
    assert_eq!(provider_tags("mistralai"), vec!["mistralai"]);
}

#[test]
fn test_matches_provider_hint_mistral_keys() {
    assert!(matches_provider_hint(
        "mistralai/mistral-large",
        Some("mistral")
    ));
    assert!(matches_provider_hint(
        "mistralai/mixtral-8x7b",
        Some("mistralai")
    ));
}

#[test]
fn test_provider_tags_ai21_with_digits() {
    assert_eq!(provider_tags("ai21"), vec!["ai21"]);
}

#[test]
fn test_matches_provider_hint_none_and_empty() {
    assert!(!matches_provider_hint("openai/gpt-4", None));
    assert!(!matches_provider_hint("openai/gpt-4", Some("")));
    assert!(!matches_provider_hint("openai/gpt-4", Some("unknown")));
}

#[test]
fn test_gjc_unknown_provider_passthrough() {
    // gjc's common providers ARE known and canonicalize as usual.
    assert_eq!(canonical_provider("anthropic"), Some("anthropic".into()));
    assert_eq!(canonical_provider("openai"), Some("openai".into()));
    assert_eq!(canonical_provider("openai-codex"), Some("openai".into()));
    assert_eq!(canonical_provider("google"), Some("google".into()));
    assert_eq!(
        canonical_provider("github-copilot"),
        Some("github_copilot".into())
    );

    // A gjc provider value that looks like a model fragment (contains
    // digits) or a placeholder is NOT treated as a provider: canonical_provider
    // yields None so the aggregator keeps the raw value verbatim rather than
    // misattributing it. This guards the unknown-provider passthrough path.
    assert_eq!(canonical_provider("gjc-model-4o"), None);
    assert_eq!(canonical_provider("<unset>"), None);
}

#[test]
fn vendor_ai_suffix_is_one_vendor() {
    // The datasets split the same DeepSeek model between two vendor
    // spellings depending on the reseller, so before this fold the tag a
    // user's usage carried was decided by who served it.
    for spelling in ["deepseek", "deepseek-ai", "deepseek_ai", "DeepSeek-AI"] {
        assert_eq!(
            canonical_provider(spelling),
            Some("deepseek".into()),
            "{spelling} must canonicalize to deepseek"
        );
    }

    // Real dataset keys, where the vendor sits in a nested segment.
    assert_eq!(
        provider_tags("nano-gpt/deepseek-ai/deepseek-v3.2-exp"),
        vec!["nano_gpt", "deepseek"]
    );
    assert_eq!(
        provider_tags("zenmux/deepseek/deepseek-v3.2-exp"),
        vec!["zenmux", "deepseek"]
    );

    assert_eq!(canonical_provider("novita-ai"), Some("novita".into()));
    assert_eq!(canonical_provider("stepfun-ai"), Some("stepfun".into()));
}

#[test]
fn regional_cn_endpoint_is_not_folded_into_the_global_one() {
    // Guards the comment above the `-ai` arms. `-cn` reads like the same
    // kind of suffix and is not: alibaba and alibaba-cn share 45 models
    // and disagree on 41, qwen-max among them at $1.60/$6.40 against
    // $0.345/$1.377. Folding these would misprice by 4.6x, so they must
    // stay distinct providers.
    assert_ne!(
        canonical_provider("alibaba-cn"),
        canonical_provider("alibaba")
    );
    assert_ne!(
        canonical_provider("siliconflow-cn"),
        canonical_provider("siliconflow")
    );
    assert_eq!(canonical_provider("alibaba-cn"), Some("alibaba_cn".into()));
}

#[test]
fn provider_spelling_match_is_exact_where_canonicalization_is_not() {
    // canonical_provider folds the two spellings together; this predicate
    // deliberately does not, so `select_best_match` can prefer the row that
    // spells the vendor the way the hint does.
    assert_eq!(
        canonical_provider("deepseek-ai"),
        canonical_provider("deepseek")
    );

    assert!(matches_provider_spelling(
        "novita/deepseek/deepseek-r1-distill-qwen-32b",
        "deepseek"
    ));
    assert!(!matches_provider_spelling(
        "cloudflare/@cf/deepseek-ai/deepseek-r1-distill-qwen-32b",
        "deepseek"
    ));

    // Case and `-`/`_` are spelling noise, not a different spelling.
    for hint in ["deepseek-ai", "deepseek_ai", "DeepSeek-AI"] {
        assert!(
            matches_provider_spelling("hyperbolic/deepseek-ai/DeepSeek-V3", hint),
            "{hint} spells the vendor the way this key does"
        );
        assert!(!matches_provider_spelling(
            "novita/deepseek/deepseek-v3-0324",
            hint
        ));
    }

    // The last segment is the model name, never a vendor spelling.
    assert!(!matches_provider_spelling("deepseek-ai", "deepseek-ai"));
    assert!(!matches_provider_spelling(
        "some-vendor/deepseek",
        "deepseek"
    ));
}

#[test]
fn provider_spelling_reads_the_dotted_prefix_of_the_final_key_component() {
    // AWS-style ids carry the provider in a dotted prefix of the final key
    // component, which is why `key_provider_tags` splits it on `.`. The
    // spelling predicate has to read the same segments, or a `deepseek`
    // hint fails to recognise the row that spells the vendor its way and
    // falls through to a differently spelled reseller.
    assert_eq!(
        key_provider_tags("amazon-bedrock/us.deepseek.r1-v1:0"),
        vec!["amazon_bedrock", "us", "deepseek"]
    );
    assert!(matches_provider_spelling(
        "amazon-bedrock/us.deepseek.r1-v1:0",
        "deepseek"
    ));
    assert!(matches_provider_spelling(
        "bedrock/us-east-1/deepseek.v3.2",
        "deepseek"
    ));
    assert!(!matches_provider_spelling(
        "amazon-bedrock/us.deepseek.r1-v1:0",
        "deepseek-ai"
    ));

    // The trailing piece is still the model name, not a vendor spelling,
    // and an undotted final component contributes nothing at all.
    assert!(!matches_provider_spelling(
        "amazon-bedrock/us.deepseek.r1-v1:0",
        "r1-v1:0"
    ));
    assert!(!matches_provider_spelling(
        "some-router/deepseek-ai",
        "deepseek-ai"
    ));
}
