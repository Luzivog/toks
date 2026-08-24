use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_parse_all_messages_refreshes_cc_mirror_provider_when_variant_metadata_changes() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let variant_dir = source_home.path().join(".cc-mirror/kimi-code");
        let config_dir = source_home.path().join("mirror-configs/kimi-code");
        let project_dir = config_dir.join("projects/project-one");
        std::fs::create_dir_all(&project_dir).unwrap();
        std::fs::create_dir_all(&variant_dir).unwrap();
        let variant_path = variant_dir.join("variant.json");
        std::fs::write(
            &variant_path,
            format!(
                r#"{{"name":"kimi-code","provider":"kimi","configDir":{}}}"#,
                paths::json_path_literal(&config_dir)
            ),
        )
        .unwrap();
        let session_path = project_dir.join("session.jsonl");
        std::fs::write(
                &session_path,
                r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#,
            )
            .unwrap();

        let first_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
        );
        assert_eq!(first_messages.len(), 1);
        assert_eq!(first_messages[0].client, "cc-mirror/kimi-code");
        assert_eq!(first_messages[0].provider_id, "kimi");

        std::fs::write(
            &variant_path,
            format!(
                r#"{{"name":"kimi-code","provider":"minimax","configDir":{}}}"#,
                paths::json_path_literal(&config_dir)
            ),
        )
        .unwrap();

        let refreshed_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
        );
        assert_eq!(refreshed_messages.len(), 1);
        assert_eq!(refreshed_messages[0].client, "cc-mirror/kimi-code");
        assert_eq!(refreshed_messages[0].provider_id, "minimax");
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_keeps_normal_claude_when_cc_mirror_points_at_claude_config() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let claude_dir = source_home.path().join(".claude");
        let project_dir = claude_dir.join("projects/project-one");
        std::fs::create_dir_all(&project_dir).unwrap();
        let session_path = project_dir.join("session.jsonl");
        std::fs::write(
                &session_path,
                r#"{"type":"assistant","timestamp":"2024-12-01T10:00:00.000Z","requestId":"req_001","message":{"id":"msg_001","model":"claude-3-5-sonnet","usage":{"input_tokens":100,"output_tokens":50}}}
"#,
            )
            .unwrap();

        let variant_dir = source_home.path().join(".cc-mirror/plain-mirror");
        std::fs::create_dir_all(&variant_dir).unwrap();
        std::fs::write(
            variant_dir.join("variant.json"),
            format!(
                r#"{{"name":"plain-mirror","provider":"mirror","configDir":{}}}"#,
                paths::json_path_literal(&claude_dir)
            ),
        )
        .unwrap();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["claude".to_string()],
            None,
        );
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].client, "claude");
    }
}
