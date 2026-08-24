use super::{support::*, *};
fn large_prime_contents(input: i64, child_input: i64) -> String {
    const FILE_BYTES: usize = 100_000;
    const SEMANTIC_OFFSET: usize = 10_000;
    let before_padding = r#"{"type":"session","version":3,"id":"legacy","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","padding":""#;
    let before_semantic = r#"","usage":{"#;
    let padding_bytes = SEMANTIC_OFFSET
        .checked_sub(before_padding.len() + before_semantic.len())
        .unwrap();
    let mut contents = String::with_capacity(FILE_BYTES);
    contents.push_str(before_padding);
    contents.push_str(&"p".repeat(padding_bytes));
    contents.push_str(before_semantic);
    assert_eq!(contents.len(), SEMANTIC_OFFSET);
    contents.push_str(&format!(
            r#""input":{input},"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":{}}}}}}}
{{"type":"child_usage_attributed","id":"usage-1","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{{"input":{child_input},"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":{child_input}}},"aggregateUsage":{{"input":{input},"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":{}}},"origin":"spawn_task"}}
"#,
            input + 10,
            input + 10,
        ));
    let tail_prefix = r#"{"type":"ignored","padding":""#;
    let tail_suffix = "\"}\n";
    let tail_bytes = FILE_BYTES
        .checked_sub(contents.len() + tail_prefix.len() + tail_suffix.len())
        .unwrap();
    contents.push_str(tail_prefix);
    contents.push_str(&"t".repeat(tail_bytes));
    contents.push_str(tail_suffix);
    assert_eq!(contents.len(), FILE_BYTES);
    contents
}

#[test]
#[serial_test::serial]
fn test_prime_agent_forked_parent_and_rlm_child_are_counted_once() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions = source_home.path().join(".prime/agent/sessions");
    let child_dir = source_home
        .path()
        .join(".prime/agent/session-artifacts/z-original/sub-child");
    std::fs::create_dir_all(&sessions).unwrap();
    std::fs::create_dir_all(&child_dir).unwrap();

    let original_path = sessions.join("z-original.jsonl");
    std::fs::write(
            sessions.join("a-fork.jsonl"),
            format!(
                r#"{{"type":"session","version":3,"id":"fork","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":0}}
{{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250}}}}}}
{{"type":"child_usage_attributed","id":"usage-1","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{{"input":30,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":40}},"aggregateUsage":{{"input":130,"output":60,"cacheRead":20,"cacheWrite":10,"totalTokens":220}},"origin":"spawn_task"}}
{{"type":"child_usage_attributed","id":"usage-2","parentId":"usage-1","timestamp":"2026-08-08T00:00:03.000Z","targetId":"parent","childUsage":{{"input":20,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":30}},"aggregateUsage":{{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250}},"origin":"spawn_task"}}
"#,
                paths::json_path_literal(&original_path)
            ),
        )
        .unwrap();
    std::fs::write(
            &original_path,
            r#"{"type":"session","version":3,"id":"original","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":100,"output":50,"cacheRead":20,"cacheWrite":10,"totalTokens":180}}}
{"type":"child_usage_attributed","id":"usage-1","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":30,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":40},"aggregateUsage":{"input":130,"output":60,"cacheRead":20,"cacheWrite":10,"totalTokens":220},"origin":"spawn_task"}
{"type":"child_usage_attributed","id":"usage-2","parentId":"usage-1","timestamp":"2026-08-08T00:00:03.000Z","targetId":"parent","childUsage":{"input":20,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":30},"aggregateUsage":{"input":150,"output":70,"cacheRead":20,"cacheWrite":10,"totalTokens":250},"origin":"spawn_task"}
"#,
        )
        .unwrap();
    std::fs::write(
            child_dir.join("child.jsonl"),
            format!(
                r#"{{"type":"session","version":3,"id":"child","timestamp":"2026-08-08T00:00:01.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message-1","parentId":null,"timestamp":"2026-08-08T00:00:02.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-response-1","usage":{{"input":30,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":40}}}}}}
{{"type":"message","id":"child-message-2","parentId":"child-message-1","timestamp":"2026-08-08T00:00:03.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-response-2","usage":{{"input":20,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":30}}}}}}
"#,
                paths::json_path_literal(&original_path)
            ),
        )
        .unwrap();

    let clients = ["prime-agent".to_string()];
    sessions::prime_agent::reset_transcript_decode_call_counts(source_home.path());
    let cold =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    let cold_decode_calls = sessions::prime_agent::transcript_decode_call_counts();
    assert_eq!(cold_decode_calls, (3, 0));

    let warm =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(
        sessions::prime_agent::transcript_decode_call_counts(),
        cold_decode_calls,
        "an unchanged warm scan must decode neither messages nor accounting"
    );

    for messages in [cold, warm] {
        assert_eq!(messages.len(), 3);
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.input)
                .sum::<i64>(),
            150
        );
        assert_eq!(
            messages
                .iter()
                .map(|message| message.tokens.output)
                .sum::<i64>(),
            70
        );
    }

    let parsed = parse_local_clients(LocalParseOptions {
        home_dir: Some(source_home.path().to_str().unwrap().to_string()),
        use_env_roots: false,
        clients: Some(clients.to_vec()),
        since: None,
        until: None,
        year: None,
        scanner_settings: scanner::ScannerSettings::default(),
    })
    .unwrap();
    assert_eq!(parsed.messages.len(), 3);
    assert_eq!(parsed.counts.get(ClientId::PrimeAgent), 3);
    assert_eq!(
        parsed
            .messages
            .iter()
            .map(|message| message.input)
            .sum::<i64>(),
        150
    );
    assert_eq!(
        parsed
            .messages
            .iter()
            .map(|message| message.output)
            .sum::<i64>(),
        70
    );
}

#[test]
#[serial_test::serial]
fn test_prime_agent_warm_cache_hashes_unsampled_semantic_rewrite() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions_dir = source_home.path().join(".prime/agent/sessions");
    let child_dir = source_home
        .path()
        .join(".prime/agent/session-artifacts/legacy/sub-child");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    std::fs::create_dir_all(&child_dir).unwrap();
    let source_path = sessions_dir.join("legacy.jsonl");
    let child_path = child_dir.join("child.jsonl");
    let old_contents = large_prime_contents(120, 20);
    let new_contents = large_prime_contents(240, 40);
    assert_eq!(old_contents.len(), new_contents.len());
    assert_eq!(&old_contents[..4_096], &new_contents[..4_096]);
    assert_eq!(&old_contents[23_976..], &new_contents[23_976..]);
    std::fs::write(&source_path, old_contents).unwrap();
    std::fs::write(
            &child_path,
            format!(
                r#"{{"type":"session","version":3,"id":"child","timestamp":"2026-08-08T00:00:01.000Z","cwd":"/tmp/project","parentSession":{},"rlmDepth":1}}
{{"type":"message","id":"child-message","parentId":null,"timestamp":"2026-08-08T00:00:02.000Z","message":{{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"child-response","usage":{{"input":40,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":40}}}}}}
"#,
                paths::json_path_literal(&source_path)
            ),
        )
        .unwrap();

    let clients = ["prime-agent".to_string()];
    let established =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(
        established
            .iter()
            .map(|message| message.tokens.input)
            .sum::<i64>(),
        160
    );

    let original_modified = std::fs::metadata(&source_path).unwrap().modified().unwrap();
    std::fs::write(&source_path, new_contents).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&source_path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();

    sessions::prime_agent::reset_transcript_decode_call_counts(source_home.path());
    let warm =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(
        sessions::prime_agent::transcript_decode_call_counts(),
        (1, 0),
        "the rewritten root is decoded once while the unchanged child stays decode-free"
    );

    let (root_messages, root_accounting) =
        sessions::prime_agent::parse_prime_agent_file_with_accounting(&source_path);
    let (child_messages, child_accounting) =
        sessions::prime_agent::parse_prime_agent_file_with_accounting(&child_path);
    let expected_cold = sessions::prime_agent::reconcile_prime_agent_messages(
        root_messages.into_iter().chain(child_messages).collect(),
        &[root_accounting, child_accounting],
    );
    assert_eq!(warm, expected_cold);
    assert_eq!(
        warm.iter().map(|message| message.tokens.input).sum::<i64>(),
        240,
        "stale accounting would fail to subtract the rewritten 40-token child aggregate"
    );
}

#[test]
#[serial_test::serial]
fn test_prime_agent_retries_when_source_changes_before_combined_parse() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions_dir = source_home.path().join(".prime/agent/sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let source_path = sessions_dir.join("parse-race.jsonl");
    std::fs::write(&source_path, large_prime_contents(120, 20)).unwrap();

    let clients = ["prime-agent".to_string()];
    let established =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(established[0].tokens.input, 120);

    std::fs::write(&source_path, large_prime_contents(360, 60)).unwrap();
    sessions::prime_agent::schedule_stable_parse_test_rewrite(
        &source_path,
        large_prime_contents(480, 80),
    );
    sessions::prime_agent::reset_transcript_decode_call_counts(source_home.path());
    let rebuilt =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(rebuilt[0].tokens.input, 480);
    assert_eq!(
        sessions::prime_agent::transcript_decode_call_counts(),
        (2, 0),
        "the first parse belongs to a different pre-parse fingerprint and must be retried"
    );

    let identity = message_cache::CacheIdentity::for_client(ClientId::PrimeAgent);
    let cached = message_cache::SourceMessageCache::load();
    let entry = cached.get(identity, &source_path).unwrap();
    assert_eq!(
        entry.fingerprint,
        message_cache::SourceFingerprint::from_path(&source_path).unwrap()
    );
    assert_eq!(entry.messages[0].tokens.input, 480);
    assert!(entry.prime_accounting.is_some());

    let decode_calls = sessions::prime_agent::transcript_decode_call_counts();
    let warm =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(warm[0].tokens.input, 480);
    assert_eq!(
        sessions::prime_agent::transcript_decode_call_counts(),
        decode_calls,
        "the exact stable retry snapshot should be a decode-free warm hit"
    );
}

#[test]
#[serial_test::serial]
fn test_prime_agent_legacy_cache_backfills_accounting_once() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions_dir = source_home.path().join(".prime/agent/sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let source_path = sessions_dir.join("legacy.jsonl");
    std::fs::write(
            &source_path,
            r#"{"type":"session","version":3,"id":"legacy","timestamp":"2026-08-08T00:00:00.000Z","cwd":"/tmp/project","rlmDepth":0}
{"type":"message","id":"parent","parentId":null,"timestamp":"2026-08-08T00:00:01.000Z","message":{"role":"assistant","provider":"anthropic","model":"claude-opus-5","responseId":"parent-response","usage":{"input":120,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":130}}}
{"type":"child_usage_attributed","id":"usage-1","parentId":"parent","timestamp":"2026-08-08T00:00:02.000Z","targetId":"parent","childUsage":{"input":20,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":20},"aggregateUsage":{"input":120,"output":10,"cacheRead":0,"cacheWrite":0,"totalTokens":130},"origin":"spawn_task"}
"#,
        )
        .unwrap();

    // Reproduce a successfully migrated v4 entry: messages and the exact
    // fingerprint survive, while the newly-added Prime accounting payload
    // is absent until the next scan backfills it.
    let identity = message_cache::CacheIdentity::for_client(ClientId::PrimeAgent);
    let messages = sessions::prime_agent::parse_prime_agent_file(&source_path);
    let legacy_fingerprint = match message_cache::SourceFingerprint::check_path_samples_only(
        &source_path,
        None,
    )
    .unwrap()
    {
        message_cache::FingerprintStatus::Changed(fingerprint) => fingerprint,
        message_cache::FingerprintStatus::Unchanged => unreachable!(),
    };
    let mut cache = message_cache::SourceMessageCache::default();
    cache.insert(message_cache::CachedSourceEntry::new(
        identity,
        &source_path,
        legacy_fingerprint,
        messages,
        Vec::new(),
        None,
    ));
    cache.save_if_dirty();

    let clients = ["prime-agent".to_string()];
    sessions::prime_agent::reset_transcript_decode_call_counts(source_home.path());
    let first =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    let first_calls = sessions::prime_agent::transcript_decode_call_counts();
    assert_eq!(first_calls, (1, 1));
    assert_eq!(first[0].tokens.input, 120);

    let second =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(
        sessions::prime_agent::transcript_decode_call_counts(),
        first_calls
    );
    assert_eq!(second[0].tokens.input, 120);
    assert!(message_cache::SourceMessageCache::load()
        .get(identity, &source_path)
        .unwrap()
        .prime_accounting
        .is_some());
}

#[test]
#[serial_test::serial]
fn test_prime_agent_legacy_backfill_rebuilds_if_source_changes_during_read() {
    let cache_home = tempfile::TempDir::new().unwrap();
    let source_home = tempfile::TempDir::new().unwrap();
    let _cache_env = redirect_cache_home(cache_home.path());
    let sessions_dir = source_home.path().join(".prime/agent/sessions");
    std::fs::create_dir_all(&sessions_dir).unwrap();
    let source_path = sessions_dir.join("legacy-race.jsonl");

    let old_contents = large_prime_contents(120, 20);
    let new_contents = large_prime_contents(240, 40);
    assert_eq!(old_contents.len(), new_contents.len());
    assert_eq!(&old_contents[..4_096], &new_contents[..4_096]);
    assert_eq!(&old_contents[23_976..], &new_contents[23_976..]);
    std::fs::write(&source_path, &old_contents).unwrap();

    let identity = message_cache::CacheIdentity::for_client(ClientId::PrimeAgent);
    let messages = sessions::prime_agent::parse_prime_agent_file(&source_path);
    let mut cache = message_cache::SourceMessageCache::default();
    cache.insert(message_cache::CachedSourceEntry::new(
        identity,
        &source_path,
        message_cache::SourceFingerprint::from_path(&source_path).unwrap(),
        messages,
        Vec::new(),
        None,
    ));
    cache.save_if_dirty();

    sessions::prime_agent::schedule_accounting_backfill_test_rewrite(&source_path, new_contents);
    sessions::prime_agent::reset_transcript_decode_call_counts(source_home.path());
    let clients = ["prime-agent".to_string()];
    let rebuilt =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    let rebuild_calls = sessions::prime_agent::transcript_decode_call_counts();
    assert_eq!(rebuild_calls, (1, 1));
    assert_eq!(rebuilt[0].tokens.input, 240);

    let warm =
        parse_all_messages_with_pricing(source_home.path().to_str().unwrap(), &clients, None);
    assert_eq!(
        sessions::prime_agent::transcript_decode_call_counts(),
        rebuild_calls
    );
    assert_eq!(warm[0].tokens.input, 240);
}
