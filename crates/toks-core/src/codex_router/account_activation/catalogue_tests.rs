use super::catalogue::{selectable_models_for_test, SelectableModel};

#[test]
fn selectable_models_follow_priority_and_preserve_advertised_efforts() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models_cache.json");
    std::fs::write(
        &path,
        br#"{"models":[
          {"slug":"second","visibility":"list","supported_in_api":true,"priority":2,"supported_reasoning_levels":[{"effort":"medium"},{"effort":"high"}]},
          {"slug":"hidden","visibility":"hide","supported_in_api":true,"priority":0,"supported_reasoning_levels":[{"effort":"low"}]},
          {"slug":"unsupported","visibility":"list","supported_in_api":false,"priority":0,"supported_reasoning_levels":[{"effort":"low"}]},
          {"slug":"first","visibility":"list","supported_in_api":true,"priority":1,"supported_reasoning_levels":[{"effort":"low"},{"effort":"ultra"}]},
          {"slug":"no-efforts","visibility":"list","supported_in_api":true,"priority":3},
          {"slug":"","visibility":"list","supported_in_api":true,"priority":0,"supported_reasoning_levels":[{"effort":"low"}]}
        ]}"#,
    )
    .unwrap();

    assert_eq!(
        selectable_models_for_test(path),
        vec![
            SelectableModel {
                slug: "first".into(),
                reasoning_efforts: vec!["low".into(), "ultra".into()],
            },
            SelectableModel {
                slug: "second".into(),
                reasoning_efforts: vec!["medium".into(), "high".into()],
            },
            SelectableModel {
                slug: "no-efforts".into(),
                reasoning_efforts: Vec::new(),
            },
        ]
    );
}

#[test]
fn missing_or_malformed_catalogue_has_no_selectable_models() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.json");
    assert!(selectable_models_for_test(missing).is_empty());

    let malformed = directory.path().join("malformed.json");
    std::fs::write(&malformed, b"not JSON").unwrap();
    assert!(selectable_models_for_test(malformed).is_empty());
}
