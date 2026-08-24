use super::{support::*, *};
#[test]
#[serial_test::serial]
fn test_source_cache_refreshes_stale_date_on_cache_hit() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let message_dir = source_home
            .path()
            .join(".local/share/opencode/storage/message/project-1");
        std::fs::create_dir_all(&message_dir).unwrap();
        let path = message_dir.join("msg_001.json");
        std::fs::write(
                &path,
                r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"accounts/fireworks/models/deepseek-v3-0324","providerID":"fireworks","cost":0,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
            )
            .unwrap();

        let fingerprint = message_cache::SourceFingerprint::from_path(&path).unwrap();
        let mut stale_message = UnifiedMessage::new(
            "opencode",
            "accounts/fireworks/models/deepseek-v3-0324",
            "fireworks",
            "session-1",
            1_733_011_200_000,
            TokenBreakdown {
                input: 10,
                output: 5,
                cache_read: 0,
                cache_write: 0,
                reasoning: 0,
            },
            0.0,
        );
        stale_message.date = "1900-01-01".to_string();

        let mut cache = message_cache::SourceMessageCache::default();
        cache.insert(message_cache::CachedSourceEntry::new(
            message_cache::CacheIdentity::for_client(ClientId::OpenCode),
            &path,
            fingerprint,
            vec![stale_message],
            Vec::new(),
            None,
        ));
        cache.save_if_dirty();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );

        assert_eq!(messages.len(), 1);
        assert_ne!(messages[0].date, "1900-01-01");
        assert_eq!(
            messages[0].date,
            UnifiedMessage::new(
                "opencode",
                "accounts/fireworks/models/deepseek-v3-0324",
                "fireworks",
                "session-1",
                1_733_011_200_000,
                TokenBreakdown {
                    input: 10,
                    output: 5,
                    cache_read: 0,
                    cache_write: 0,
                    reasoning: 0,
                },
                0.0,
            )
            .date
        );
    }
}

#[cfg(unix)]
#[test]
#[serial_test::serial]
fn test_empty_parse_results_are_not_cached_for_optional_file_sources() {
    use std::os::unix::fs::PermissionsExt;

    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let message_dir = source_home
            .path()
            .join(".local/share/opencode/storage/message/project-1");
        std::fs::create_dir_all(&message_dir).unwrap();
        let path = message_dir.join("msg_001.json");
        std::fs::write(
                &path,
                r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"accounts/fireworks/models/deepseek-v3-0324","providerID":"fireworks","cost":0,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
            )
            .unwrap();

        let mut permissions = std::fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o000);
        std::fs::set_permissions(&path, permissions).unwrap();

        let first_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert!(first_messages.is_empty());

        let cache = message_cache::SourceMessageCache::load();
        assert!(cache
            .get(
                message_cache::CacheIdentity::for_client(ClientId::OpenCode),
                &path,
            )
            .is_none());

        let mut readable_permissions = std::fs::metadata(&path).unwrap().permissions();
        readable_permissions.set_mode(0o644);
        std::fs::set_permissions(&path, readable_permissions).unwrap();

        let second_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert_eq!(second_messages.len(), 1);
    }
}

#[test]
#[serial_test::serial]
fn test_empty_cache_hits_are_reparsed_for_optional_file_sources() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let message_dir =
            client_scan_root(source_home.path(), ClientId::OpenCode).join("project-1");
        std::fs::create_dir_all(&message_dir).unwrap();
        let path = message_dir.join("msg_001.json");
        std::fs::write(
                &path,
                r#"{"id":"msg-1","sessionID":"session-1","role":"assistant","modelID":"accounts/fireworks/models/deepseek-v3-0324","providerID":"fireworks","cost":0,"tokens":{"input":10,"output":5,"reasoning":0,"cache":{"read":0,"write":0}},"time":{"created":1733011200000}}"#,
            )
            .unwrap();

        let fingerprint = message_cache::SourceFingerprint::from_path(&path).unwrap();
        let mut cache = message_cache::SourceMessageCache::default();
        cache.insert(message_cache::CachedSourceEntry::new(
            message_cache::CacheIdentity::for_client(ClientId::OpenCode),
            &path,
            fingerprint,
            Vec::new(),
            Vec::new(),
            None,
        ));
        cache.save_if_dirty();

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert_eq!(messages.len(), 1);

        let loaded = message_cache::SourceMessageCache::load();
        let repaired_entry = loaded
            .get(
                message_cache::CacheIdentity::for_client(ClientId::OpenCode),
                &path,
            )
            .unwrap();
        assert_eq!(repaired_entry.messages.len(), 1);
    }
}

#[test]
#[serial_test::serial]
fn test_sqlite_source_cache_invalidates_on_wal_change() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let db_dir = source_home.path().join(".local/share/opencode");
        std::fs::create_dir_all(&db_dir).unwrap();
        let db_path = db_dir.join("opencode.db");

        let conn = rusqlite::Connection::open(&db_path).unwrap();
        let journal_mode: String = conn
            .query_row("PRAGMA journal_mode=WAL;", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_lowercase(), "wal");
        conn.execute_batch(
            "PRAGMA wal_autocheckpoint=0;
                 CREATE TABLE message (
                     id TEXT PRIMARY KEY,
                     session_id TEXT NOT NULL,
                     data TEXT NOT NULL
                 );",
        )
        .unwrap();

        let row_one = r#"{
                "role": "assistant",
                "modelID": "claude-sonnet-4",
                "providerID": "anthropic",
                "tokens": { "input": 100, "output": 50, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
                "time": { "created": 1700000000000.0 }
            }"#;
        let row_two = r#"{
                "role": "assistant",
                "modelID": "claude-sonnet-4",
                "providerID": "anthropic",
                "tokens": { "input": 120, "output": 60, "reasoning": 0, "cache": { "read": 0, "write": 0 } },
                "time": { "created": 1700000001000.0 }
            }"#;

        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params!["msg-1", "session-1", row_one],
        )
        .unwrap();

        let first_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert_eq!(first_messages.len(), 1);

        conn.execute(
            "INSERT INTO message (id, session_id, data) VALUES (?1, ?2, ?3)",
            rusqlite::params!["msg-2", "session-1", row_two],
        )
        .unwrap();
        assert!(db_path.with_extension("db-wal").exists());

        let refreshed_messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["opencode".to_string()],
            None,
        );
        assert_eq!(refreshed_messages.len(), 2);
    }
}
