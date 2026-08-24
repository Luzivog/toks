use super::*;

#[test]
fn test_devin_parser_versions_invalidate_stale_entries() {
    assert_eq!(parser_version(ClientId::DevinCli), 3);
    assert_eq!(parser_version(ClientId::DevinDesktop), 3);
}

#[test]
fn test_codex_version_bumps_without_retiring_claude_history() {
    assert_eq!(parser_version(ClientId::Codex), 9);
    assert_eq!(parser_version(ClientId::Claude), 2);
}

#[test]
fn test_copilot_parser_version_invalidates_stale_entries() {
    assert_eq!(parser_version(ClientId::Copilot), 9);
}

#[test]
fn test_duration_anchor_audit_remaining_parsers_bumps_versions() {
    // Follow-up to #890: junie, jcode, devin-cli, zcode, and
    // opencodereview were re-anchored to start-anchored duration
    // timestamps; their cache-invalidating parser versions must bump so
    // stale end-anchored-timestamp cache entries are not reused.
    //
    // Second-round review found gaps in that first pass: zcode's
    // NULL-`started_at` fallback stayed end-anchored and its
    // `is_turn_start` marking didn't follow the new start-anchored
    // timestamps, and kiro's structured messages.jsonl turns stayed
    // end-anchored when the prompt timestamp was missing. Both bump
    // again here so those stale (start-anchored-but-still-wrong) v2/v1
    // cache entries are also invalidated.
    assert_eq!(parser_version(ClientId::Junie), 4);
    assert_eq!(parser_version(ClientId::Jcode), 8);
    assert_eq!(parser_version(ClientId::DevinCli), 3);
    assert_eq!(parser_version(ClientId::Zcode), 4);
    assert_eq!(parser_version(ClientId::OpenCodeReview), 4);
    assert_eq!(parser_version(ClientId::Kiro), 3);
}

#[test]
fn test_kimi_parser_version_invalidates_stale_entries() {
    assert_eq!(parser_version(ClientId::Kimi), 5);
}

#[test]
fn test_hermes_parser_version_invalidates_v1_entries() {
    assert_eq!(parser_version(ClientId::Hermes), 2);
}

#[test]
fn test_grok_resilient_line_reader_parser_version_invalidates_v6_entries() {
    // A Grok session file that is never appended to again keeps its
    // fingerprint forever, so only the version bump discards the truncated
    // v6 parse and forces a cold reparse.
    assert_eq!(parser_version(ClientId::Grok), 7);
}

#[test]
fn test_micode_parser_version_invalidates_rows_without_cost_provenance() {
    assert_eq!(parser_version(ClientId::MiMoCode), 3);
}

#[test]
fn test_junie_parser_version_invalidates_rows_without_cost_provenance() {
    assert_eq!(parser_version(ClientId::Junie), 4);
}

#[test]
#[serial_test::serial]
fn test_kimi_stale_parser_cache_is_rejected_and_rebuilt_with_same_fingerprint() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source_home = TempDir::new().unwrap();
    // Spelled the way the scan will spell it. `ClientDef::resolve_path`
    // joins the root with `/` and `WalkDir` appends the components below it
    // with the platform separator, so on Windows the parse stores this
    // entry under `<home>/.kimi/sessions\group\session\wire.jsonl` while a
    // `Path::join` fixture asks for it back under all backslashes.
    // `CachedPath` keys on the OS string as written, so those are two keys
    // for one file and the lookup below found nothing.
    let wire_path = PathBuf::from(
        ClientId::Kimi
            .data()
            .resolve_path_with_env_strategy(&source_home.path().to_string_lossy(), false),
    )
    .join("group")
    .join("session")
    .join("wire.jsonl");
    std::fs::create_dir_all(wire_path.parent().unwrap()).unwrap();
    std::fs::write(
        &wire_path,
        concat!(
            r#"{"type":"metadata","protocol_version":"1.3"}"#,
            "\n",
            r#"{"timestamp":1770983410.0,"message":{"type":"StatusUpdate","payload":{"token_usage":{"input_other":9223372036854775807,"output":9223372036854775807,"input_cache_read":2,"input_cache_creation":0},"message_id":"msg-extreme"}}}"#,
            "\n",
        ),
    )
    .unwrap();

    let fingerprint =
        match SourceFingerprint::check_kimi_path_samples_only(&wire_path, None).unwrap() {
            FingerprintStatus::Changed(fingerprint) => fingerprint,
            FingerprintStatus::Unchanged => panic!("an uncached source must build a fingerprint"),
        };
    let identity = CacheIdentity::for_client(ClientId::Kimi);
    let stale_identity = CacheIdentity {
        namespace: identity.namespace,
        parser_version: identity.parser_version.saturating_sub(1),
    };
    let stale_message = UnifiedMessage::new(
        "kimi",
        "stale-model",
        "moonshot",
        "stale-session",
        1,
        TokenBreakdown {
            input: 999,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
    );
    let stale_entry = CachedSourceEntry::new(
        stale_identity,
        &wire_path,
        fingerprint.clone(),
        vec![stale_message],
        Vec::new(),
        None,
    );
    let stale_shard = cache_shard_path(identity, &wire_path);
    ensure_cache_dir(stale_shard.parent().unwrap()).unwrap();
    write_shard_with_limit(
        &stale_shard,
        stale_identity,
        &[stale_entry],
        MAX_CACHE_SHARD_BYTES,
    )
    .unwrap();

    let loaded = SourceMessageCache::load();
    assert!(loaded.get(identity, &wire_path).is_none());
    assert!(matches!(
        SourceFingerprint::check_kimi_path_samples_only(&wire_path, Some(&fingerprint)),
        Some(FingerprintStatus::Unchanged)
    ));

    let first = crate::parse_all_messages_with_pricing_with_env_strategy(
        source_home.path().to_str().unwrap(),
        &["kimi".to_string()],
        None,
        false,
        &crate::scanner::ScannerSettings::default(),
    );
    assert_eq!(first.len(), 1);
    assert_eq!(first[0].tokens.input, i64::MAX);
    assert_eq!(first[0].tokens.output, i64::MAX);
    assert_eq!(first[0].tokens.cache_read, 2);
    assert_eq!(first[0].tokens.cache_write, 0);
    assert!(
        matches!(
            SourceFingerprint::check_kimi_path_samples_only(&wire_path, Some(&fingerprint)),
            Some(FingerprintStatus::Unchanged)
        ),
        "parser-version invalidation must not require a source rewrite"
    );

    let rebuilt = SourceMessageCache::load();
    let cached = rebuilt
        .get(identity, &wire_path)
        .expect("production loader should persist the reparsed Kimi entry");
    assert_eq!(cached.parser_version, identity.parser_version);
    assert_eq!(cached.fingerprint, fingerprint);
    assert_eq!(cached.messages.len(), 1);
    assert_eq!(cached.messages[0].tokens.input, i64::MAX);
    assert_eq!(cached.messages[0].tokens.output, i64::MAX);
    assert_eq!(cached.messages[0].tokens.cache_read, 2);
    assert_eq!(cached.messages[0].tokens.cache_write, 0);

    let second = crate::parse_all_messages_with_pricing_with_env_strategy(
        source_home.path().to_str().unwrap(),
        &["kimi".to_string()],
        None,
        false,
        &crate::scanner::ScannerSettings::default(),
    );
    assert_eq!(second, first);
}

#[test]
#[serial_test::serial]
fn test_copilot_stale_cache_is_rejected_and_rebuilt_with_root_agent() {
    let temp_home = TempDir::new().unwrap();
    let _cache_env = sandbox_cache_env(temp_home.path());
    let source_dir = TempDir::new().unwrap();
    let source_path = source_dir.path().join("copilot-otel.jsonl");
    std::fs::write(
        &source_path,
        concat!(
            r#"{"type":"span","traceId":"trace-cache","spanId":"invoke-sub","parentSpanId":"tool-task","name":"invoke_agent","attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.agent.id":"github.copilot.subagent"}}"#,
            "\n",
            r#"{"type":"span","traceId":"trace-cache","spanId":"tool-task","parentSpanId":"invoke-root","name":"execute_tool task"}"#,
            "\n",
            r#"{"type":"span","traceId":"trace-cache","spanId":"invoke-root","name":"invoke_agent","attributes":{"gen_ai.operation.name":"invoke_agent","gen_ai.agent.id":"github.copilot.default"}}"#,
            "\n",
            r#"{"type":"span","traceId":"trace-cache","spanId":"chat","parentSpanId":"invoke-root","name":"chat gpt-5.4-mini","attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":1,"gen_ai.usage.output_tokens":1}}"#,
            "\n",
            r#"{"type":"span","traceId":"trace-cache","spanId":"chat","parentSpanId":"invoke-root","name":"chat gpt-5.4-mini","attributes":{"gen_ai.operation.name":"chat","gen_ai.request.model":"gpt-5.4-mini","gen_ai.response.model":"gpt-5.4-mini","gen_ai.usage.input_tokens":9,"gen_ai.usage.output_tokens":8}}"#,
            "\n",
        ),
    )
    .unwrap();

    let current_identity = CacheIdentity::for_client(ClientId::Copilot);
    let stale_identity = CacheIdentity {
        namespace: current_identity.namespace,
        parser_version: current_identity.parser_version.saturating_sub(1),
    };
    let mut stale_message = UnifiedMessage::new_with_dedup(
        "copilot",
        "gpt-5.4-mini",
        "github-copilot",
        "trace-cache",
        1,
        TokenBreakdown {
            input: 1,
            output: 1,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some("trace-cache:chat".to_string()),
    );
    stale_message.agent = Some("github.copilot.subagent".to_string());
    let stale_duplicate = UnifiedMessage::new_with_dedup(
        "copilot",
        "gpt-5.4-mini",
        "github-copilot",
        "trace-cache",
        2,
        TokenBreakdown {
            input: 9,
            output: 8,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        0.0,
        Some("trace-cache:chat".to_string()),
    );
    let fingerprint = SourceFingerprint::from_path(&source_path).unwrap();
    let stale_entry = CachedSourceEntry::new(
        stale_identity,
        &source_path,
        fingerprint.clone(),
        vec![stale_message, stale_duplicate],
        Vec::new(),
        None,
    );
    let shard_key = CacheKey::new(current_identity, &source_path).shard();
    let stale_path = shard_path(&cache_shard_dir().unwrap(), &shard_key);
    ensure_cache_dir(stale_path.parent().unwrap()).unwrap();
    write_shard_with_limit(
        &stale_path,
        stale_identity,
        &[stale_entry],
        MAX_CACHE_SHARD_BYTES,
    )
    .unwrap();

    let mut loaded = SourceMessageCache::load();
    assert!(
        loaded.get(current_identity, &source_path).is_none(),
        "a stale Copilot cache entry must not be served after the parser output change"
    );
    assert!(loaded.rewrite_shards.contains(&shard_key));
    assert_eq!(
        SourceFingerprint::from_path(&source_path).unwrap(),
        fingerprint,
        "the source fingerprint must remain unchanged; parser version causes invalidation"
    );

    let rebuilt = crate::sessions::copilot::parse_copilot_file(&source_path);
    assert_eq!(rebuilt.len(), 1);
    assert_eq!(rebuilt[0].dedup_key.as_deref(), Some("trace-cache:chat"));
    assert_eq!(rebuilt[0].tokens.input, 9);
    assert_eq!(rebuilt[0].tokens.output, 8);
    assert_eq!(
        rebuilt[0].agent.as_deref(),
        Some("github.copilot.default"),
        "a cold rebuild must use the root invoke_agent attribution"
    );
    loaded.insert(CachedSourceEntry::new(
        current_identity,
        &source_path,
        fingerprint,
        rebuilt,
        Vec::new(),
        None,
    ));
    loaded.save_if_dirty();

    let reloaded = SourceMessageCache::load();
    let cached = reloaded
        .get(current_identity, &source_path)
        .expect("rebuilt Copilot cache entry should survive reload");
    assert_eq!(cached.parser_version, current_identity.parser_version);
    assert_eq!(
        cached.messages[0].agent.as_deref(),
        Some("github.copilot.default")
    );
    assert!(matches!(
        read_shard(&stale_path, current_identity),
        ShardReadStatus::Loaded(entries)
            if entries.len() == 1
                && entries[0].messages[0].tokens.input == 9
                && entries[0].messages[0].agent.as_deref()
                    == Some("github.copilot.default")
    ));
}
