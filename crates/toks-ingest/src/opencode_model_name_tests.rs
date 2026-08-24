use super::{
    global_config_dir, load_for_home, load_with_environment, EnvironmentConfig,
    OpenCodeModelNameResolver,
};

fn config_with_name(name: &str) -> String {
    format!(
        r#"{{"provider":{{"fireworks-ai":{{"models":{{"accounts/fireworks/models/glm-5p2":{{"name":"{name}"}}}}}}}}}}"#
    )
}

#[test]
fn uses_configured_name_for_matching_provider_and_model() {
    let resolver = OpenCodeModelNameResolver::from_json(
        r#"{
            "provider": {
                "fireworks-ai": {
                    "models": {
                        "accounts/fireworks/models/glm-5p2": { "name": "GLM 5.2" }
                    }
                }
            }
        }"#,
    );

    assert_eq!(
        resolver.display_name("fireworks_ai", "accounts/fireworks/models/glm-5p2"),
        Some("GLM 5.2")
    );
    assert_eq!(
        resolver.display_name("fireworks_ai", "accounts/fireworks/models/glm-5p1"),
        None
    );
}

#[test]
fn skips_missing_or_blank_names_without_rejecting_other_models() {
    let resolver = OpenCodeModelNameResolver::from_json(
        r#"{
            "provider": {
                "fireworks-ai": {
                    "models": {
                        "missing": {},
                        "blank": { "name": "  " },
                        "named": { "name": "DeepSeek V4 Flash" }
                    }
                }
            }
        }"#,
    );

    assert_eq!(resolver.display_name("fireworks-ai", "missing"), None);
    assert_eq!(resolver.display_name("fireworks-ai", "blank"), None);
    assert_eq!(
        resolver.display_name("fireworks-ai", "named"),
        Some("DeepSeek V4 Flash")
    );
}

#[test]
fn reads_jsonc_global_config_with_comments_and_trailing_commas() {
    let home = tempfile::TempDir::new().unwrap();
    let config_dir = home.path().join(".config/opencode");
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        config_dir.join("opencode.jsonc"),
        r#"{
            // Model labels are for display only.
            "provider": {
                "fireworks-ai": {
                    "models": {
                        "accounts/fireworks/models/glm-5p2": {
                            "name": "GLM 5.2",
                        },
                    },
                },
            },
        }"#,
    )
    .unwrap();

    let resolver = load_for_home(Some(home.path()));

    assert_eq!(
        resolver.display_name("fireworks", "accounts/fireworks/models/glm-5p2"),
        Some("GLM 5.2")
    );
}

#[test]
fn explicit_home_ignores_environment_config_sources() {
    let home = tempfile::TempDir::new().unwrap();
    let home_config_dir = home.path().join(".config/opencode");
    std::fs::create_dir_all(&home_config_dir).unwrap();
    std::fs::write(
        home_config_dir.join("opencode.json"),
        r#"{"provider":{"fireworks-ai":{"models":{"accounts/fireworks/models/glm-5p2":{"name":"Home Config"}}}}}"#,
    )
    .unwrap();

    let legacy_config_dir = home.path().join(".opencode");
    std::fs::create_dir_all(&legacy_config_dir).unwrap();
    std::fs::write(
        legacy_config_dir.join("opencode.json"),
        r#"{"provider":{"fireworks-ai":{"models":{"accounts/fireworks/models/glm-5p2":{"name":"Home Legacy"}}}}}"#,
    )
    .unwrap();

    let resolver = load_with_environment(home.path(), None);

    assert_eq!(
        resolver.display_name("fireworks", "accounts/fireworks/models/glm-5p2"),
        Some("Home Legacy")
    );
}

#[test]
fn normal_runs_apply_xdg_and_environment_sources_in_opencode_order() {
    let home = tempfile::TempDir::new().unwrap();
    let xdg = tempfile::TempDir::new().unwrap();
    let external = tempfile::TempDir::new().unwrap();
    let xdg_config_dir = xdg.path().join("opencode");
    let legacy_config_dir = home.path().join(".opencode");
    let config_dir = external.path().join("config-dir");
    let custom_config = external.path().join("custom.json");
    std::fs::create_dir_all(&xdg_config_dir).unwrap();
    std::fs::create_dir_all(&legacy_config_dir).unwrap();
    std::fs::create_dir_all(&config_dir).unwrap();
    std::fs::write(
        xdg_config_dir.join("opencode.json"),
        config_with_name("XDG"),
    )
    .unwrap();
    std::fs::write(&custom_config, config_with_name("Custom")).unwrap();
    std::fs::write(
        legacy_config_dir.join("opencode.json"),
        config_with_name("Legacy"),
    )
    .unwrap();
    std::fs::write(
        config_dir.join("opencode.json"),
        config_with_name("Config Dir"),
    )
    .unwrap();

    let environment = EnvironmentConfig {
        xdg_config_home: Some(xdg.path().to_path_buf()),
        custom_config: Some(custom_config),
        custom_config_dir: Some(config_dir),
        inline_config: Some(config_with_name("Inline")),
    };

    assert_eq!(
        global_config_dir(home.path(), environment.xdg_config_home.as_deref()),
        xdg_config_dir
    );
    let resolver = load_with_environment(home.path(), Some(&environment));

    assert_eq!(
        resolver.display_name("fireworks", "accounts/fireworks/models/glm-5p2"),
        Some("Inline")
    );
}

#[test]
fn trailing_comma_with_comment_then_close_brace_is_valid_jsonc() {
    let resolver = OpenCodeModelNameResolver::from_json(
        r#"{
            "provider": {
                "fireworks-ai": {
                    "models": {
                        "accounts/fireworks/models/glm-5p2": {
                            "name": "GLM 5.2", // model label comment
                        },
                    },
                },
            },
        }"#,
    );

    assert_eq!(
        resolver.display_name("fireworks", "accounts/fireworks/models/glm-5p2"),
        Some("GLM 5.2")
    );
}

#[test]
fn colliding_canonical_ids_resolve_to_single_name() {
    let resolver = OpenCodeModelNameResolver::from_json(
        r#"{
            "provider": {
                "fireworks": {
                    "models": {
                        "glm-5p2-20250701": { "name": "Dated" },
                        "glm-5p2": { "name": "Short" }
                    }
                }
            }
        }"#,
    );

    // Both keys canonicalize to "glm-5p2" (date suffix stripped),
    // so both lookups return the same name.
    let dated = resolver.display_name("fireworks", "glm-5p2-20250701");
    let short = resolver.display_name("fireworks", "glm-5p2");
    assert!(dated.is_some());
    assert_eq!(dated, short);
}
