use crate::codex_router::account_activation::catalogue::ModelChoice;

const NOW: i64 = 1_800_000_000_000;

#[test]
fn command_is_exact_account_isolated_read_only_and_four_letter_prompt() {
    let model = ModelChoice {
        slug: Some("best-model".into()),
        reasoning: "low".into(),
    };
    let command = crate::codex_router::account_activation::command::command_for_test(
        std::path::Path::new("/opt/codex"),
        std::path::Path::new("/home/person"),
        std::path::Path::new("/profiles/a/.codex"),
        &model,
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert_eq!(args.last().map(String::as_str), Some("test"));
    assert_eq!(
        crate::codex_router::account_activation::command::PROMPT_FOR_TEST,
        "test"
    );
    for expected in [
        "--ignore-user-config",
        "--ignore-rules",
        "read-only",
        "service_tier=\"default\"",
        "model_reasoning_effort=\"low\"",
    ] {
        assert!(args.iter().any(|arg| arg == expected), "missing {expected}");
    }
    let environment = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        environment["CODEX_HOME"].as_deref(),
        Some("/profiles/a/.codex")
    );
    assert!(!environment.contains_key("OPENAI_API_KEY"));
}

#[test]
fn manual_command_uses_the_router_capability_without_an_account_header() {
    let model = ModelChoice {
        slug: Some("best-model".into()),
        reasoning: "low".into(),
    };
    let command = crate::codex_router::account_activation::command::manual_command_for_test(
        std::path::Path::new("/opt/codex"),
        std::path::Path::new("/home/person"),
        std::path::Path::new("/home/person/.codex"),
        "00000000-0000-4000-8000-000000000051",
        &model,
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(args
        .iter()
        .any(|arg| arg == "model_provider=\"toks_activation\""));
    assert!(args.iter().any(|arg| {
        arg == "model_providers.toks_activation.env_http_headers={\"x-toks-activation-attempt\"=\"TOKS_ACTIVATION_ATTEMPT\"}"
    }));
    assert!(args
        .iter()
        .any(|arg| arg == "model_providers.toks_activation.supports_websockets=false"));
    let environment = command
        .get_envs()
        .map(|(name, value)| {
            (
                name.to_string_lossy().into_owned(),
                value.map(|value| value.to_string_lossy().into_owned()),
            )
        })
        .collect::<std::collections::BTreeMap<_, _>>();
    assert_eq!(
        environment["TOKS_ACTIVATION_ATTEMPT"].as_deref(),
        Some("00000000-0000-4000-8000-000000000051")
    );
    assert_eq!(
        environment["CODEX_HOME"].as_deref(),
        Some("/home/person/.codex")
    );
    assert!(!args.iter().any(|arg| arg.contains("chatgpt-account-id")));
}

#[test]
fn catalogue_chooses_primary_visible_model_and_lowest_effort() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models_cache.json");
    std::fs::write(&path, br#"{"fetched_at":"2027-01-15T08:00:00Z","models":[
      {"slug":"hidden","visibility":"hide","supported_in_api":true,"priority":0,"supported_reasoning_levels":[{"effort":"low"}]},
      {"slug":"second","visibility":"list","supported_in_api":true,"priority":2,"supported_reasoning_levels":[{"effort":"medium"}]},
      {"slug":"best","visibility":"list","supported_in_api":true,"priority":1,"supported_reasoning_levels":[{"effort":"high"},{"effort":"low"}]}
    ]}"#).unwrap();
    assert_eq!(
        crate::codex_router::account_activation::catalogue::best_for_test(path, NOW),
        Some(ModelChoice {
            slug: Some("best".into()),
            reasoning: "low".into()
        })
    );
}

#[test]
fn stale_or_other_account_catalogue_is_not_imposed_on_the_selected_account() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("models_cache.json");
    std::fs::write(
        &path,
        br#"{"fetched_at":"2026-01-01T00:00:00Z","models":[
          {"slug":"stale-best","visibility":"list","supported_in_api":true,"priority":1,"supported_reasoning_levels":[{"effort":"low"}]}
        ]}"#,
    )
    .unwrap();
    assert_eq!(
        crate::codex_router::account_activation::catalogue::best_for_test(path, NOW),
        None
    );

    let choice =
        crate::codex_router::account_activation::catalogue::best_for_profile(directory.path());
    assert_eq!(choice.slug, None);
    assert_eq!(choice.reasoning, "low");
    let command = crate::codex_router::account_activation::command::command_for_test(
        std::path::Path::new("/opt/codex"),
        std::path::Path::new("/home/person"),
        directory.path(),
        &choice,
    );
    let args = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    assert!(!args.iter().any(|arg| arg == "-m"));
    assert_eq!(args.last().map(String::as_str), Some("test"));
}
