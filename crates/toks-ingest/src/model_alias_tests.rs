use super::*;

fn resolver(pairs: &[(&str, &str)]) -> ModelAliasResolver {
    let entries = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    ModelAliasResolver::from_config(&ModelAliasMap { entries })
}

#[test]
fn folds_three_variants_to_one_canonical() {
    let r = resolver(&[
        ("claude-opus-4-8-cc", "claude-opus-4-8"),
        ("anthropic/claude-opus-4-8", "claude-opus-4-8"),
    ]);
    // All three real-world spellings collapse to the canonical name. The
    // third needs no map entry: syntactic normalization already lowercases it.
    for input in [
        "claude-opus-4-8-cc",
        "anthropic/claude-opus-4-8",
        "Claude-Opus-4-8",
    ] {
        assert_eq!(
            r.apply(crate::normalize_syntactic(input)),
            "claude-opus-4-8",
            "input {input} should fold to claude-opus-4-8"
        );
    }
}

#[test]
fn keys_match_case_and_dotted_insensitively() {
    // Config key written with upper case and a dotted version still matches
    // the normalized input, because both sides run through normalize_syntactic.
    let r = resolver(&[("Claude-Opus-4.8-CC", "claude-opus-4-8")]);
    assert_eq!(
        r.apply(crate::normalize_syntactic("claude-opus-4-8-cc")),
        "claude-opus-4-8"
    );
}

#[test]
fn drops_empty_and_self_maps() {
    let r = resolver(&[
        ("", "claude-opus-4-8"),
        ("claude-opus-4-8-cc", ""),
        ("gpt-5.5", "gpt-5.5"),
    ]);
    assert!(r.map.is_empty());
}

#[test]
fn resolution_is_single_hop() {
    // {a: b, b: c} resolves a -> b (not c) and never loops.
    let r = resolver(&[("model-a", "model-b"), ("model-b", "model-c")]);
    assert_eq!(r.apply("model-a".to_string()), "model-b");
    assert_eq!(r.apply("model-b".to_string()), "model-c");
}

#[test]
fn separator_insensitive_match_is_provider_agnostic() {
    // Finding A: `normalize_syntactic` rewrites `.`→`-` only for claude, so
    // the resolver must fold separators itself for every other provider. The
    // regression is when the CONFIGURED alias key and the model string the
    // provider actually reports use different separators — the old exact
    // HashMap lookup missed and left the variant unfolded.

    // Dashed alias key (`gpt-5-5-cc`), dotted model spelling (`gpt-5.5-cc`):
    // must still fold to the canonical `gpt-5.5`.
    let dashed_key = resolver(&[("gpt-5-5-cc", "gpt-5.5")]);
    assert_eq!(
        dashed_key.apply(crate::normalize_syntactic("gpt-5.5-cc")),
        "gpt-5.5",
        "a dashed alias key must match the dotted model spelling (gpt-5-5 ↔ gpt-5.5)"
    );

    // Mirror: dotted alias key, dashed model spelling.
    let dotted_key = resolver(&[("gpt-5.5-cc", "gpt-5.5")]);
    assert_eq!(
        dotted_key.apply(crate::normalize_syntactic("gpt-5-5-cc")),
        "gpt-5.5",
        "a dotted alias key must match the dashed model spelling"
    );
}

#[test]
fn miss_is_identity() {
    let r = resolver(&[("claude-opus-4-8-cc", "claude-opus-4-8")]);
    assert_eq!(r.apply("gpt-5.5".to_string()), "gpt-5.5");
}

#[test]
fn empty_resolver_is_identity() {
    let r = ModelAliasResolver::default();
    assert_eq!(
        r.apply("claude-opus-4-8-cc".to_string()),
        "claude-opus-4-8-cc"
    );
}

#[test]
fn respects_capacity_cap() {
    let entries: BTreeMap<String, String> = (0..MAX_MODEL_ALIASES + 100)
        .map(|i| (format!("alias-{i}"), format!("canonical-{i}")))
        .collect();
    let r = ModelAliasResolver::from_config(&ModelAliasMap { entries });
    assert_eq!(r.map.len(), MAX_MODEL_ALIASES);
}

#[test]
fn deserialize_is_lossy_over_non_string_values() {
    // Non-string values are skipped; string entries survive.
    let parsed: ModelAliasMap =
        serde_json::from_str(r#"{"a": "b", "n": 5, "arr": ["x"]}"#).unwrap();
    assert_eq!(parsed.entries.len(), 1);
    assert_eq!(parsed.entries.get("a").map(String::as_str), Some("b"));
}

#[test]
fn deserialize_of_non_object_is_empty() {
    // A misuse (array/scalar instead of an object) degrades to empty, not error.
    assert!(serde_json::from_str::<ModelAliasMap>("[]")
        .unwrap()
        .entries
        .is_empty());
    assert!(serde_json::from_str::<ModelAliasMap>("\"oops\"")
        .unwrap()
        .entries
        .is_empty());
}

#[test]
fn serialize_round_trips_as_flat_map() {
    let map = ModelAliasMap {
        entries: [(
            "claude-opus-4-8-cc".to_string(),
            "claude-opus-4-8".to_string(),
        )]
        .into_iter()
        .collect(),
    };
    let json = serde_json::to_string(&map).unwrap();
    assert_eq!(json, r#"{"claude-opus-4-8-cc":"claude-opus-4-8"}"#);
    assert_eq!(serde_json::from_str::<ModelAliasMap>(&json).unwrap(), map);
}
