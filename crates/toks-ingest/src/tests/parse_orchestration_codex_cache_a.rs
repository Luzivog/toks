use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_codex_cache_reparses_from_zero_when_incremental_prefix_is_stale() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let fresh_cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut cache_env = redirect_cache_home(cache_home.path());

    {
        let codex_dir = client_scan_root(source_home.path(), ClientId::Codex);
        std::fs::create_dir_all(&codex_dir).unwrap();
        let path = codex_dir.join("session.jsonl");
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
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
        assert_eq!(initial_messages[0].model_id, "gpt-5.4");
        assert!(message_cache::SourceMessageCache::load()
            .get(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            )
            .and_then(|entry| entry.codex_incremental.as_ref())
            .is_some());

        std::thread::sleep(std::time::Duration::from_millis(5));
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.5"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":15,"cached_input_tokens":3,"output_tokens":5},"last_token_usage":{"input_tokens":5,"cached_input_tokens":1,"output_tokens":2}}}}"#,
                    "\n"
                ),
            )
            .unwrap();

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
        assert!(warm_messages
            .iter()
            .all(|message| message.model_id == "gpt-5.5"));
    }
}

#[test]
#[serial_test::serial]
fn test_source_cache_keeps_untimestamped_rows_in_sync_after_append() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let fresh_cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut cache_env = redirect_cache_home(cache_home.path());

    {
        let codex_dir = source_home.path().join(".codex/sessions");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let path = codex_dir.join("session.jsonl");
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                    "\n"
                ),
            )
            .unwrap();

        let first_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );
        assert_eq!(first_messages.len(), 1);

        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(
                concat!(
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
    }
}

#[test]
#[serial_test::serial]
fn test_source_cache_matches_cold_parse_after_malformed_json_append() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let fresh_cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let mut cache_env = redirect_cache_home(cache_home.path());

    {
        let codex_dir = source_home.path().join(".codex/sessions");
        std::fs::create_dir_all(&codex_dir).unwrap();
        let path = codex_dir.join("session.jsonl");
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":999""#,
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

        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        file.write_all(
                concat!(
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
        assert!(message_cache::SourceMessageCache::load()
            .get(
                message_cache::CacheIdentity::for_client(ClientId::Codex),
                &path,
            )
            .is_none());

        point_cache_home(&mut cache_env, fresh_cache_home.path());
        let fresh_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(warm_messages, fresh_messages);
    }
}

#[test]
#[serial_test::serial]
fn test_exact_hit_codex_cache_repairs_fallback_timestamps_without_incremental_state() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let session_dir = source_home.path().join(".codex/sessions");
        std::fs::create_dir_all(&session_dir).unwrap();
        let path = session_dir.join("session.jsonl");
        std::fs::write(
                &path,
                concat!(
                    r#"{"type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                    "\n",
                    r#"{"type":"event_msg","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3},"last_token_usage":{"input_tokens":10,"cached_input_tokens":2,"output_tokens":3}}}}"#,
                    "\n"
                ),
            )
            .unwrap();

        let expected = crate::sessions::codex::parse_codex_file(&path);
        assert_eq!(expected.len(), 1);

        let fingerprint = message_cache::SourceFingerprint::from_path(&path).unwrap();
        let mut stale_message = expected[0].clone();
        stale_message.timestamp = 0;
        stale_message.date = "1900-01-01".to_string();

        let mut cache = message_cache::SourceMessageCache::default();
        cache.insert(message_cache::CachedSourceEntry::new(
            message_cache::CacheIdentity::for_client(ClientId::Codex),
            &path,
            fingerprint,
            vec![stale_message],
            vec![0],
            None,
        ));
        cache.save_if_dirty();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["codex".to_string()],
            None,
        );

        assert_eq!(messages, expected);
    }
}
