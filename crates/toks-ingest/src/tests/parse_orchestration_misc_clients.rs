use super::{support::*, *};
pub(super) fn write_kimi_repeated_status_fixture(source_home: &std::path::Path) {
    let session_dir = source_home.join(".kimi/sessions/group-1/session-1");
    std::fs::create_dir_all(&session_dir).unwrap();
    std::fs::write(
            session_dir.join("wire.jsonl"),
            r#"{"type": "metadata", "protocol_version": "1.3"}
{"timestamp": 1770983410.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 10, "output": 1, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-progressive"}}}
{"timestamp": 1770983420.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 20, "output": 2, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-progressive"}}}
{"timestamp": 1770983430.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 5, "output": 1, "input_cache_read": 0, "input_cache_creation": 0}, "message_id": "msg-distinct"}}}
{"timestamp": 1770983440.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 7, "output": 1, "input_cache_read": 0, "input_cache_creation": 0}}}}
{"timestamp": 1770983450.0, "message": {"type": "StatusUpdate", "payload": {"token_usage": {"input_other": 8, "output": 1, "input_cache_read": 0, "input_cache_creation": 0}}}}"#,
        )
        .unwrap();
}

fn write_kimchi_fixture(path: &std::path::Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
            path,
            r#"{"type":"session","id":"kimchi-session","timestamp":"2026-08-01T00:00:00.000Z","cwd":"/tmp/kimchi-project"}
{"type":"message","id":"kimchi-message","timestamp":"2026-08-01T00:00:01.000Z","message":{"role":"assistant","model":"kimi-k2.6","provider":"kimchi-dev","usage":{"input":100,"output":10,"cacheRead":5,"cacheWrite":2,"totalTokens":117}}}"#,
        )
        .unwrap();
}
fn write_cline_cli_fixture(path: &std::path::Path, messages: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(
        path,
        format!(r#"{{"sessionId":"cline-dedup-session","messages":[{messages}]}}"#),
    )
    .unwrap();
}

#[test]
#[serial_test::serial]
fn test_cline_cli_deduplicates_duplicate_records_in_cached_and_local_paths() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        let duplicate = r#"{"id":"duplicate","role":"assistant","ts":1785320475705,"modelInfo":{"id":"cline-free/glm-5.2","provider":"cline-pass"},"metrics":{"inputTokens":100,"outputTokens":10}}"#;
        let distinct_a = r#"{"id":"distinct-a","role":"assistant","ts":1785320476705,"metrics":{"inputTokens":200,"outputTokens":20}}"#;
        let distinct_b = r#"{"id":"distinct-b","role":"assistant","ts":1785320477705,"metrics":{"inputTokens":300,"outputTokens":30}}"#;
        write_cline_cli_fixture(
            &source_home
                .path()
                .join(".cline/data/sessions/first/first.messages.json"),
            &format!("{duplicate},{distinct_a}"),
        );
        write_cline_cli_fixture(
            &source_home
                .path()
                .join(".cline/data/sessions/second/second.messages.json"),
            &format!("{duplicate},{distinct_b}"),
        );

        let clients = ["cline".to_string()];
        let scanner_settings = scanner::ScannerSettings::default();
        let cached = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &clients,
            None,
            false,
            &scanner_settings,
        );
        let mut cached_inputs = cached
            .iter()
            .map(|message| message.tokens.input)
            .collect::<Vec<_>>();
        cached_inputs.sort_unstable();
        assert_eq!(cached_inputs, vec![100, 200, 300]);
        let cached_again = parse_all_messages_with_pricing_with_env_strategy(
            source_home.path().to_str().unwrap(),
            &clients,
            None,
            false,
            &scanner_settings,
        );
        let mut cached_again_inputs = cached_again
            .iter()
            .map(|message| message.tokens.input)
            .collect::<Vec<_>>();
        cached_again_inputs.sort_unstable();
        assert_eq!(cached_again_inputs, vec![100, 200, 300]);

        let parsed = parse_local_clients(LocalParseOptions {
            home_dir: Some(source_home.path().to_str().unwrap().to_string()),
            use_env_roots: false,
            clients: Some(clients.to_vec()),
            since: None,
            until: None,
            year: None,
            scanner_settings,
        })
        .unwrap();
        let mut local_inputs = parsed
            .messages
            .iter()
            .map(|message| message.input)
            .collect::<Vec<_>>();
        local_inputs.sort_unstable();
        assert_eq!(local_inputs, vec![100, 200, 300]);
        assert_eq!(parsed.counts.get(ClientId::Cline), 3);
    }
}

#[test]
#[serial_test::serial]
fn test_parse_all_messages_with_pricing_kimi_deduplicates_repeated_status_updates() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_kimi_repeated_status_fixture(source_home.path());

        let messages = parse_all_messages_with_pricing(
            source_home.path().to_str().unwrap(),
            &["kimi".to_string()],
            None,
        );

        assert_eq!(messages.len(), 4);
        assert_eq!(messages.iter().map(|m| m.tokens.input).sum::<i64>(), 40);
        assert_eq!(messages.iter().map(|m| m.tokens.output).sum::<i64>(), 5);
    }
}

#[test]
#[serial_test::serial]
fn test_kimchi_deduplicates_same_message_across_scan_roots() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    let default_path = source_home
        .path()
        .join(".config/kimchi/harness/sessions/workspace/session.jsonl");
    let extra_path = source_home
        .path()
        .join("kimchi-extra/workspace/session.jsonl");
    write_kimchi_fixture(&default_path);
    write_kimchi_fixture(&extra_path);

    let mut extra_scan_paths = std::collections::BTreeMap::new();
    extra_scan_paths.insert(
        "kimchi".to_string(),
        vec![source_home.path().join("kimchi-extra")],
    );
    let scanner_settings = scanner::ScannerSettings {
        extra_scan_paths,
        ..Default::default()
    };

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(source_home.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["kimchi".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner_settings.clone(),
    })
    .unwrap();
    assert_eq!(parsed.counts.get(ClientId::Kimchi), 1);
    assert_eq!(parsed.messages.len(), 1);

    let messages = parse_all_messages_with_pricing_with_env_strategy(
        source_home.path().to_str().unwrap(),
        &["kimchi".to_string()],
        None,
        false,
        &scanner_settings,
    );
    assert_eq!(messages.len(), 1);
    assert_eq!(
        messages[0].dedup_key.as_deref(),
        Some("kimchi:kimchi-session:kimchi-message")
    );
}
