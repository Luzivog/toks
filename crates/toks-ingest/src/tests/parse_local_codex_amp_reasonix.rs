use super::{codex_fixtures::*, support::*, *};
#[test]
#[serial_test::serial]
fn test_parse_local_clients_codex_counts_deduplicated_forked_history() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());

    {
        write_codex_forked_history_fixture(source_home.path());

        let parsed = parse_local_clients(LocalParseOptions {
            home_dir: Some(source_home.path().to_str().unwrap().to_string()),
            use_env_roots: false,
            clients: Some(vec!["codex".to_string()]),
            since: None,
            until: None,
            year: None,
            scanner_settings: scanner::ScannerSettings::default(),
        })
        .unwrap();

        assert_eq!(parsed.counts.get(ClientId::Codex), 3);
        assert_eq!(parsed.messages.len(), 3);
        assert_eq!(
            parsed
                .messages
                .iter()
                .map(|message| message.input)
                .sum::<i64>(),
            88
        );
        assert_eq!(
            parsed
                .messages
                .iter()
                .map(|message| message.cache_read)
                .sum::<i64>(),
            22
        );
        assert_eq!(
            parsed
                .messages
                .iter()
                .map(|message| message.output)
                .sum::<i64>(),
            33
        );
    }
}

#[test]
fn test_parse_local_clients_amp_partial_ledger_recovers_message_fallback_day() {
    use chrono::TimeZone;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let amp_dir = temp_dir.path().join(".local/share/amp/threads");
    std::fs::create_dir_all(&amp_dir).unwrap();

    let thread_created = chrono::DateTime::parse_from_rfc3339("2026-04-04T12:00:00Z")
        .unwrap()
        .timestamp_millis();
    let ledger_timestamp = chrono::DateTime::parse_from_rfc3339("2026-04-08T12:00:00Z")
        .unwrap()
        .timestamp_millis();

    let thread = format!(
        r#"{{
                "id": "thread-amp-gap",
                "created": {thread_created},
                "usageLedger": {{
                    "events": [
                        {{
                            "timestamp": "2026-04-08T12:00:00Z",
                            "model": "claude-sonnet-4-0",
                            "credits": 0.75,
                            "tokens": {{ "input": 100, "output": 20 }}
                        }}
                    ]
                }},
                "messages": [
                    {{
                        "role": "assistant",
                        "messageId": 1,
                        "usage": {{
                            "model": "claude-sonnet-4-0",
                            "inputTokens": 100,
                            "outputTokens": 20,
                            "credits": 0.75
                        }}
                    }},
                    {{
                        "role": "assistant",
                        "messageId": 2,
                        "usage": {{
                            "model": "claude-sonnet-4-0",
                            "inputTokens": 50,
                            "outputTokens": 10,
                            "credits": 0.40
                        }}
                    }}
                ]
            }}"#
    );
    std::fs::write(amp_dir.join("T-thread-amp-gap.json"), thread).unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["amp".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    assert_eq!(parsed.counts.get(ClientId::Amp), 2);
    assert_eq!(parsed.messages.len(), 2);

    let dates: HashSet<String> = parsed.messages.iter().map(|msg| msg.date.clone()).collect();
    let local_date = |timestamp_ms: i64| {
        chrono::Local
            .timestamp_millis_opt(timestamp_ms)
            .single()
            .unwrap()
            .format("%Y-%m-%d")
            .to_string()
    };
    assert!(dates.contains(&local_date(thread_created + 2000)));
    assert!(dates.contains(&local_date(ledger_timestamp)));
}

#[test]
fn test_parse_local_clients_reasonix_counts_reported_requests() {
    let temp_dir = tempfile::TempDir::new().unwrap();
    // Where the scan will actually look, rather than the Unix spelling of
    // it: under an explicit home Reasonix lives at `~/.reasonix` on Unix
    // and `%HOME%\AppData\Roaming\reasonix` on Windows, so a hardcoded
    // `.reasonix/stats` fixture is written somewhere the scanner never
    // reads and the test asserts on an empty parse. The path layout has its
    // own coverage in `clients::tests`; this test is about the request
    // count.
    let stats_dir = std::path::PathBuf::from(
        ClientId::Reasonix
            .data()
            .resolve_path_with_env_strategy(&temp_dir.path().to_string_lossy(), false),
    );
    std::fs::create_dir_all(&stats_dir).unwrap();
    std::fs::write(
            stats_dir.join("2026-08-04.jsonl"),
            "{\"ts\":\"2026-08-04T09:10:11Z\",\"model\":\"deepseek/chat\",\"prompt\":100,\"completion\":20,\"total\":120,\"requests\":3}\n",
        )
        .unwrap();

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(temp_dir.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(vec!["reasonix".to_string()]),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();

    assert_eq!(parsed.messages.len(), 1);
    assert_eq!(parsed.messages[0].message_count, 3);
    assert_eq!(parsed.counts.get(ClientId::Reasonix), 3);
}
