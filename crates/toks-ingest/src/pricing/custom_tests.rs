use super::*;
use tempfile::TempDir;

fn pricing(input: f64, output: f64) -> ModelPricing {
    ModelPricing {
        input_cost_per_token: Some(input),
        output_cost_per_token: Some(output),
        ..Default::default()
    }
}

#[test]
fn loads_empty_when_file_missing() {
    let temp = TempDir::new().unwrap();
    let loaded = CustomPricing::load_from_path(&temp.path().join("missing.json"));

    assert!(loaded.lookup("anything").is_none());
}

#[test]
fn loads_valid_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "$schema": "https://tokscope.ai/custom-pricing.schema.json",
            "models": {
                    "accounts/fireworks/routers/kimi-k2p6-turbo": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00,
                    "cache_read_input_token_cost_per_million_tokens": 0.30,
                    "source": "https://docs.fireworks.ai/serverless/pricing",
                    "notes": "Fireworks Kimi K2.6 Turbo"
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);
    let pricing = loaded
        .lookup("accounts/fireworks/routers/kimi-k2p6-turbo")
        .unwrap();

    assert_eq!(loaded.len(), 1);
    assert_eq!(pricing.input_cost_per_token, Some(0.000002));
    assert_eq!(pricing.output_cost_per_token, Some(0.000008));
    assert_eq!(pricing.cache_read_input_token_cost, Some(0.0000003));
}

#[test]
fn loads_litellm_per_token_fields() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "copy-pasted": {
                    "input_cost_per_token": 0.000002,
                    "output_cost_per_token": 0.000008,
                    "cache_read_input_token_cost": 0.0000003,
                    "source": "copied from LiteLLM-shaped JSON"
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);
    let pricing = loaded.lookup("copy-pasted").unwrap();

    assert_eq!(pricing.input_cost_per_token, Some(0.000002));
    assert_eq!(pricing.output_cost_per_token, Some(0.000008));
    assert_eq!(pricing.cache_read_input_token_cost, Some(0.0000003));
}

#[test]
fn loads_mixed_per_million_and_litellm_per_token_entries() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "per-million": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00
                },
                "per-token": {
                    "input_cost_per_token": 0.00000095,
                    "output_cost_per_token": 0.000004
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert_eq!(
        loaded.lookup("per-million").unwrap().input_cost_per_token,
        Some(0.000002)
    );
    assert_eq!(
        loaded.lookup("per-token").unwrap().input_cost_per_token,
        Some(0.00000095)
    );
}

#[test]
fn drops_entry_when_per_million_and_per_token_alias_both_set() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "ambiguous": {
                    "input_cost_per_million_tokens": 2.00,
                    "input_cost_per_token": 0.000002,
                    "output_cost_per_million_tokens": 8.00
                },
                "good": {
                    "input_cost_per_million_tokens": 1.00,
                    "output_cost_per_million_tokens": 4.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert!(loaded.lookup("ambiguous").is_none());
    assert_eq!(
        loaded.lookup("good").unwrap().input_cost_per_token,
        Some(0.000001)
    );
}

#[test]
fn rejects_out_of_range_json_number_before_loading_entries() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "too-large": {
                    "input_cost_per_million_tokens": 1e500,
                    "output_cost_per_million_tokens": 8.00
                },
                "good": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert!(loaded.is_empty());
}

#[test]
fn tolerates_malformed_json() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(&path, r#"{"models": {"#).unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert!(loaded.is_empty());
    assert!(loaded.lookup("model").is_none());
}

#[test]
fn tolerates_malformed_entry_keeps_others() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "bad": {
                    "input_cost_per_million_tokens": "not-a-number",
                    "output_cost_per_million_tokens": 8.00
                },
                "good": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert!(loaded.lookup("bad").is_none());
    assert_eq!(
        loaded.lookup("good").unwrap().input_cost_per_token,
        Some(0.000002)
    );
}

#[test]
fn keeps_entries_with_input_or_output_price() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "missing-output": {
                    "input_cost_per_million_tokens": 2.00
                },
                "missing-input": {
                    "output_cost_per_million_tokens": 8.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert_eq!(
        loaded
            .lookup("missing-output")
            .unwrap()
            .input_cost_per_token,
        Some(0.000002)
    );
    assert_eq!(
        loaded
            .lookup("missing-input")
            .unwrap()
            .output_cost_per_token,
        Some(0.000008)
    );
}

#[test]
fn drops_entries_with_no_input_or_output_prices() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "cache-only": {
                    "cache_read_input_token_cost_per_million_tokens": 0.30
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert!(loaded.lookup("cache-only").is_none());
}

#[test]
fn keeps_free_models_but_still_drops_negative_prices() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "all-zero": {
                    "input_cost_per_million_tokens": 0.0,
                    "output_cost_per_million_tokens": 0.0
                },
                "free-input": {
                    "input_cost_per_million_tokens": 0.0,
                    "output_cost_per_million_tokens": 8.00
                },
                "negative-output": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": -8.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    // A free model is the case this file exists to let users express when
    // no upstream dataset publishes the model (#1021). 0.0 is an assertion
    // ("free"), not an absence, so an all-zero row is kept.
    let all_zero = loaded.lookup("all-zero").expect("free model must load");
    assert_eq!(all_zero.input_cost_per_token, Some(0.0));
    assert_eq!(all_zero.output_cost_per_token, Some(0.0));

    assert_eq!(
        loaded.lookup("free-input").unwrap().output_cost_per_token,
        Some(0.000008)
    );
    // Unchanged: a negative rate is nonsense, not a statement about price.
    assert!(loaded.lookup("negative-output").is_none());
}

#[test]
fn rejects_non_finite_prices() {
    assert!(validate_non_negative(f64::NAN, "input").is_err());
    assert!(validate_non_negative(f64::INFINITY, "input").is_err());
    assert!(validate_non_negative(f64::NEG_INFINITY, "input").is_err());
}

#[test]
fn ignores_unknown_bookkeeping_fields() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "annotated": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00,
                    "source": "https://example.com/pricing",
                    "notes": "kept for the user, ignored by Toks"
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert_eq!(
        loaded.lookup("annotated").unwrap().input_cost_per_token,
        Some(0.000002)
    );
}

#[test]
fn drops_oversized_file() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    let file = fs::File::create(&path).unwrap();
    file.set_len(MAX_CUSTOM_PRICING_FILE_BYTES + 1).unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert!(loaded.is_empty());
}

#[test]
fn case_insensitive_lookup() {
    let mut models = HashMap::new();
    models.insert("MiXeD-Model".to_string(), pricing(0.000002, 0.000008));
    let loaded = CustomPricing::from_models(models);

    assert_eq!(
        loaded.lookup("mixed-model").unwrap().input_cost_per_token,
        Some(0.000002)
    );
    assert_eq!(
        loaded.lookup("MIXED-MODEL").unwrap().output_cost_per_token,
        Some(0.000008)
    );
}

#[test]
fn duplicate_keys_last_wins() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "Model-A": {
                    "input_cost_per_million_tokens": 1.00,
                    "output_cost_per_million_tokens": 4.00
                },
                "model-a": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert_eq!(
        loaded.lookup("MODEL-A").unwrap().input_cost_per_token,
        Some(0.000002)
    );
}

#[test]
fn literal_duplicate_keys_last_wins() {
    let temp = TempDir::new().unwrap();
    let path = temp.path().join("custom-pricing.json");
    fs::write(
        &path,
        r#"{
            "models": {
                "model-a": {
                    "input_cost_per_million_tokens": 1.00,
                    "output_cost_per_million_tokens": 4.00
                },
                "model-a": {
                    "input_cost_per_million_tokens": 2.00,
                    "output_cost_per_million_tokens": 8.00
                }
            }
        }"#,
    )
    .unwrap();

    let loaded = CustomPricing::load_from_path(&path);

    assert_eq!(
        loaded.lookup("model-a").unwrap().input_cost_per_token,
        Some(0.000002)
    );
}
