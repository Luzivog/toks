use super::catalogue::Catalogue;

/// Shaped like a real `models_cache.json`: Fast-capable models list a tier,
/// the small ones carry an empty `service_tiers`.
fn cache() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(
        dir.path().join("models_cache.json"),
        r#"{"fetched_at":"2026-08-22T19:28:45Z","models":[
            {"slug":"gpt-5.6-sol","service_tiers":[{"id":"ultrafast"},{"id":"priority","name":"Fast"}],
             "additional_speed_tiers":["fast"]},
            {"slug":"gpt-future","service_tiers":[{"id":""}],
             "additional_speed_tiers":["fast"]},
            {"slug":"gpt-5.3-codex-spark","service_tiers":[],"additional_speed_tiers":[]}
        ]}"#,
    )
    .expect("write cache");
    dir
}

#[test]
fn advertised_tier_is_used_and_absent_tiers_are_declined() {
    let dir = cache();
    let catalogue = Catalogue::at(Some(dir.path().join("models_cache.json")));
    assert_eq!(catalogue.fast_tier("gpt-5.6-sol"), Some("priority"));
    assert_eq!(catalogue.fast_tier("gpt-future"), Some("priority"));
    // Spark advertises no speed tier; injecting one risks failing the turn.
    assert_eq!(catalogue.fast_tier("gpt-5.3-codex-spark"), None);
    // A model the catalogue has never heard of is treated the same way.
    assert_eq!(catalogue.fast_tier("gpt-9-unreleased"), None);
}

#[test]
fn unreadable_catalogue_declines_rather_than_guessing() {
    assert_eq!(Catalogue::at(None).fast_tier("gpt-5.6-sol"), None);
    let dir = tempfile::tempdir().expect("temp dir");
    let missing = Catalogue::at(Some(dir.path().join("models_cache.json")));
    assert_eq!(missing.fast_tier("gpt-5.6-sol"), None);
}
