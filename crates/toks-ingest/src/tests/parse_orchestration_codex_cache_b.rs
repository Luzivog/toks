use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_codex_cache_repairs_fallback_timestamps_after_source_mtime_change() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let fresh_cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home.path().join(".codex/sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");
        let contents = concat!(
            r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
            "\n",
            r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
            "\n"
        );
        std::fs::write(&path, contents).unwrap();

        let initial_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );
        assert_eq!(initial_messages.len(), 1);

        std::thread::sleep(std::time::Duration::from_millis(20));
        std::fs::write(&path, contents).unwrap();

        let warm_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        point_cache_home(&mut cache_env, fresh_cache_home.path());
        let fresh_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(warm_messages, fresh_messages);
        assert_ne!(warm_messages[0].timestamp, initial_messages[0].timestamp);
    }
}

#[test]
#[serial_test::serial]
fn test_full_log_parse_preserves_valid_messages_before_invalid_line_error() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home.path().join(".codex/sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");

        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .unwrap();
        file.write_all(&[0xff, b'\n']).unwrap();
        file.flush().unwrap();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].model_id, "gpt-5.4");

        let cache = message_cache::SourceMessageCache::load();
        assert!(cache
            .get(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            )
            .is_none());
    }
}

#[test]
#[serial_test::serial]
fn test_codex_cache_does_not_persist_unknown_before_later_turn_context() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let fresh_cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = client_scan_root(source_home.path(), ClientId::Codex);
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"session_meta","payload":{"source":"interactive","model_provider":"openai"}}"#,
                    "\n",
                    r#"{"timestamp":"2026-04-27T10:00:00Z","type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                    "\n"
                ),
            )
            .unwrap();

        let initial_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );
        assert_eq!(initial_messages.len(), 1);
        assert_eq!(initial_messages[0].model_id, "unknown");
        assert!(message_cache::SourceMessageCache::load()
            .get(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            )
            .is_none());

        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(
                concat!(
                    r#"{"timestamp":"2026-04-27T10:00:04Z","type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .unwrap();
        file.flush().unwrap();

        let resumed_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        point_cache_home(&mut cache_env, fresh_cache_home.path());
        let fresh_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(resumed_messages, fresh_messages);
        assert_eq!(resumed_messages.len(), 1);
        assert_eq!(resumed_messages[0].model_id, "gpt-5.5");

        point_cache_home(&mut cache_env, cache_home.path());
        assert!(message_cache::SourceMessageCache::load()
            .get(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            )
            .is_some());
    }
}

#[test]
#[serial_test::serial]
fn test_codex_cache_skips_non_newline_terminated_resume_prefix() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let fresh_cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home.path().join(".codex/sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#
                ),
            )
            .unwrap();

        let initial_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );
        assert_eq!(initial_messages.len(), 1);
        assert!(message_cache::SourceMessageCache::load()
            .get(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            )
            .is_none());

        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(
                concat!(
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15,"cached_input_tokens":3,"output_tokens":5},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2}}}}"#,
                    "\n"
                )
                .as_bytes(),
            )
            .unwrap();
        file.flush().unwrap();

        let warm_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        point_cache_home(&mut cache_env, fresh_cache_home.path());
        let fresh_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(warm_messages, fresh_messages);
        assert_eq!(warm_messages.len(), 2);
    }
}
