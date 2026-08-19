use super::legacy_wire::{self, LegacyCachedSourceEntryV5, LegacyUnifiedMessageV5, FORMAT_V5};
use super::{
    build_codex_incremental_cache, cache_shard_dir, ensure_cache_dir, load_codex_accounting_seed,
    reset_shard_read_count, shard_path, shard_read_count, CacheIdentity, CacheKey,
    CachedShardEnvelope, CachedSourceEntry, SourceFingerprint, SourceMessageCache,
    MAX_CACHE_SHARD_BYTES,
};
use crate::clients::ClientId;
use crate::sessions::codex::CodexParseState;
use crate::{TokenBreakdown, UnifiedMessage};
use bincode::Options;
use std::fs::{self, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;

fn write_v5_codex_shard(
    source: &Path,
    messages: Vec<UnifiedMessage>,
    fallback_timestamp_indices: Vec<usize>,
    state: CodexParseState,
    consumed_offset: u64,
) {
    let identity = CacheIdentity::for_client(ClientId::Codex);
    let incremental = build_codex_incremental_cache(source, consumed_offset, state).unwrap();
    let entry = CachedSourceEntry::new(
        identity,
        source,
        SourceFingerprint::from_path(source).unwrap(),
        messages,
        fallback_timestamp_indices,
        Some(incremental),
    );
    let key = CacheKey::from_entry(&entry);
    let shard = shard_path(&cache_shard_dir().unwrap(), &key.shard());
    ensure_cache_dir(shard.parent().unwrap()).unwrap();
    let legacy_entry = LegacyCachedSourceEntryV5 {
        parser_namespace: entry.parser_namespace,
        parser_version: identity.parser_version - 1,
        path: entry.path,
        fingerprint: entry.fingerprint,
        messages: entry
            .messages
            .into_iter()
            .map(LegacyUnifiedMessageV5::from)
            .collect(),
        fallback_timestamp_indices: entry.fallback_timestamp_indices,
        codex_incremental: entry.codex_incremental.map(Into::into),
        prime_accounting: None,
    };
    let envelope = CachedShardEnvelope {
        format_version: FORMAT_V5,
        parser_namespace: identity.namespace.to_string(),
        parser_version: identity.parser_version - 1,
        payload: bincode::options().serialize(&vec![legacy_entry]).unwrap(),
    };
    let mut writer = BufWriter::new(std::fs::File::create(shard).unwrap());
    bincode::options()
        .serialize_into(&mut writer, &envelope)
        .unwrap();
    writer.flush().unwrap();
}

fn token_line(timestamp: &str, input: i64, last_input: i64) -> String {
    format!(
        concat!(
            r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":2}},"last_token_usage":{{"input_tokens":{last_input},"cached_input_tokens":0,"output_tokens":1}}}}}}}}"#,
            "\n"
        ),
        timestamp = timestamp,
        input = input,
        last_input = last_input
    )
}

fn padded_token_line(timestamp: &str, input: i64, padding_bytes: usize) -> String {
    let mut line = serde_json::json!({
        "timestamp": timestamp,
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": {
                "total_token_usage": {
                    "input_tokens": input,
                    "cached_input_tokens": 0,
                    "output_tokens": 2
                },
                "last_token_usage": {
                    "input_tokens": 6,
                    "cached_input_tokens": 0,
                    "output_tokens": 1
                },
                "padding": "x".repeat(padding_bytes)
            }
        }
    })
    .to_string();
    line.push('\n');
    line
}

#[test]
fn v5_claude_payload_migrates_without_losing_retained_messages() {
    let mut source = tempfile::NamedTempFile::new().unwrap();
    source.write_all(b"{}\n").unwrap();
    source.flush().unwrap();
    let identity = CacheIdentity::for_client(ClientId::Claude);
    let entry = CachedSourceEntry::new(
        identity,
        source.path(),
        SourceFingerprint::from_path(source.path()).unwrap(),
        vec![UnifiedMessage::new_with_dedup(
            "claude",
            "claude-test",
            "anthropic",
            "retained-session",
            1,
            TokenBreakdown::default(),
            0.0,
            Some("msg:req".to_string()),
        )],
        Vec::new(),
        None,
    );
    let legacy_entry = LegacyCachedSourceEntryV5 {
        parser_namespace: entry.parser_namespace,
        parser_version: entry.parser_version,
        path: entry.path,
        fingerprint: entry.fingerprint,
        messages: entry
            .messages
            .into_iter()
            .map(LegacyUnifiedMessageV5::from)
            .collect(),
        fallback_timestamp_indices: entry.fallback_timestamp_indices,
        codex_incremental: entry.codex_incremental.map(Into::into),
        prime_accounting: entry.prime_accounting,
    };
    let payload = bincode::options().serialize(&vec![legacy_entry]).unwrap();
    let entries = legacy_wire::decode(FORMAT_V5, &payload, MAX_CACHE_SHARD_BYTES)
        .unwrap()
        .unwrap();

    assert_eq!(entries.len(), 1);
    let message = &entries[0].messages[0];
    assert_eq!(message.dedup_key.as_deref(), Some("msg:req"));
    assert!(message.durable_identity.is_none());
    assert!(message.accounting_aliases.is_empty());
}

#[test]
#[serial_test::serial]
fn v5_codex_incremental_state_is_marked_legacy_for_accounting() {
    let config = tempfile::TempDir::new().unwrap();
    let mut env = crate::paths::test_env::EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", config.path());
    let mut source = tempfile::NamedTempFile::new().unwrap();
    source.write_all(b"{}\n").unwrap();
    source.flush().unwrap();
    let message = UnifiedMessage::new_with_dedup(
        "codex",
        "gpt-5.4",
        "openai",
        "legacy-session",
        1,
        TokenBreakdown::default(),
        0.0,
        Some("legacy-event".to_string()),
    );
    let state = CodexParseState {
        current_model: Some("gpt-5.4".to_string()),
        ..Default::default()
    };
    write_v5_codex_shard(
        source.path(),
        vec![message],
        Vec::new(),
        state,
        source.as_file().metadata().unwrap().len(),
    );

    let seed = load_codex_accounting_seed(source.path()).unwrap();
    assert!(seed.legacy_identity_state);
}

#[test]
#[serial_test::serial]
fn prior_v5_prefix_and_suffix_cold_reparse_keep_current_identities() {
    let config = tempfile::TempDir::new().unwrap();
    let home = tempfile::TempDir::new().unwrap();
    let checkpoint = tempfile::TempDir::new().unwrap();
    let mut env = crate::paths::test_env::EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", config.path());
    let source = home
        .path()
        .join(".codex/sessions/2026/08/19/session-identity.jsonl");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        format!(
            "{}{}{}",
            concat!(
                r#"{"timestamp":"2026-08-19T00:00:00Z","type":"session_meta","payload":{"id":"session-identity","source":"interactive","model_provider":"openai"}}"#,
                "\n"
            ),
            concat!(
                r#"{"timestamp":"2026-08-19T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                "\n"
            ),
            token_line("2026-08-19T00:00:02Z", 10, 10)
        ),
    )
    .unwrap();
    let prefix = crate::sessions::codex::parse_codex_file_incremental(
        &source,
        0,
        CodexParseState::default(),
    );
    assert_eq!(prefix.messages.len(), 1);
    write_v5_codex_shard(
        &source,
        prefix.messages,
        prefix.fallback_timestamp_indices,
        prefix.state,
        prefix.consumed_offset,
    );
    OpenOptions::new()
        .append(true)
        .open(&source)
        .unwrap()
        .write_all(token_line("2026-08-19T00:00:02Z", 16, 6).as_bytes())
        .unwrap();

    let options = crate::accounting_delta::AccountingDeltaOptions {
        home_dir: Some(home.path().to_string_lossy().into_owned()),
        use_env_roots: false,
        ..Default::default()
    };
    let mut collector =
        crate::accounting_delta::AccountingDeltaCollector::open_at(checkpoint.path()).unwrap();
    let first = collector.collect(options.clone(), None).unwrap();
    assert_eq!(first.sources[0].observations.len(), 2);
    assert!(first.sources[0]
        .observations
        .iter()
        .all(|message| message.durable_identity.is_none()));
    assert!(!first.sources[0].backfill_complete);
    collector.commit(&first).unwrap();

    let upgraded = collector.collect(options.clone(), None).unwrap();
    assert_eq!(upgraded.sources[0].observations.len(), 2);
    let revision = upgraded.sources[0].revision.clone();
    let identities: Vec<_> = upgraded.sources[0]
        .observations
        .iter()
        .map(|message| message.durable_identity.clone())
        .collect();
    assert!(identities.iter().all(Option::is_some));
    assert_ne!(identities[0], identities[1]);
    collector.commit(&upgraded).unwrap();
    drop(collector);

    let state_path = checkpoint.path().join("accounting-checkpoints-v1.json");
    let mut state: serde_json::Value =
        serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    state["sources"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap()["parser_version"] = serde_json::json!(0);
    fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();

    let mut reopened =
        crate::accounting_delta::AccountingDeltaCollector::open_at(checkpoint.path()).unwrap();
    let reparsed = reopened.collect(options, None).unwrap();
    assert_eq!(reparsed.sources[0].revision, revision);
    let reparsed_identities: Vec<_> = reparsed.sources[0]
        .observations
        .iter()
        .map(|message| message.durable_identity.clone())
        .collect();
    assert_eq!(reparsed_identities, identities);
}

#[test]
#[serial_test::serial]
fn legacy_frontier_publishes_a_multichunk_suffix_before_strong_replay() {
    let config = tempfile::TempDir::new().unwrap();
    let home = tempfile::TempDir::new().unwrap();
    let checkpoint = tempfile::TempDir::new().unwrap();
    let mut env = crate::paths::test_env::EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", config.path());
    let source = home
        .path()
        .join(".codex/sessions/2026/08/19/session-large-suffix.jsonl");
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(
        &source,
        format!(
            "{}{}{}",
            concat!(
                r#"{"timestamp":"2026-08-19T00:00:00Z","type":"session_meta","payload":{"id":"session-large-suffix","source":"interactive","model_provider":"openai"}}"#,
                "\n"
            ),
            concat!(
                r#"{"timestamp":"2026-08-19T00:00:01Z","type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
                "\n"
            ),
            token_line("2026-08-19T00:00:02Z", 10, 10)
        ),
    )
    .unwrap();
    let prefix = crate::sessions::codex::parse_codex_file_incremental(
        &source,
        0,
        CodexParseState::default(),
    );
    write_v5_codex_shard(
        &source,
        prefix.messages,
        prefix.fallback_timestamp_indices,
        prefix.state,
        prefix.consumed_offset,
    );
    let mut file = OpenOptions::new().append(true).open(&source).unwrap();
    for (timestamp, total) in [
        ("2026-08-19T00:00:03Z", 16),
        ("2026-08-19T00:00:04Z", 22),
        ("2026-08-19T00:00:05Z", 28),
    ] {
        file.write_all(padded_token_line(timestamp, total, 4 * 1024 * 1024).as_bytes())
            .unwrap();
    }
    file.flush().unwrap();

    let options = crate::accounting_delta::AccountingDeltaOptions {
        home_dir: Some(home.path().to_string_lossy().into_owned()),
        use_env_roots: false,
        ..Default::default()
    };
    let mut collector =
        crate::accounting_delta::AccountingDeltaCollector::open_at(checkpoint.path()).unwrap();
    let first = collector.collect(options.clone(), None).unwrap();
    assert_eq!(first.sources[0].observations.len(), 2);
    assert!(first.sources[0]
        .observations
        .iter()
        .all(|message| message.durable_identity.is_none()));
    collector.commit(&first).unwrap();
    let second = collector.collect(options.clone(), None).unwrap();
    assert_eq!(second.sources[0].observations.len(), 1);
    let second_timestamp = second.sources[0].observations[0].timestamp;
    collector.commit(&second).unwrap();
    let tail = collector.collect(options.clone(), None).unwrap();
    assert_eq!(tail.sources[0].observations.len(), 1);
    assert!(tail.sources[0].observations[0].timestamp > second_timestamp);
    assert!(tail.sources[0].observations[0].durable_identity.is_none());
    assert!(!tail.sources[0].backfill_complete);
    collector.commit(&tail).unwrap();

    let mut strong_observations = 0;
    let mut replay_complete = false;
    for _ in 0..4 {
        let replay = collector.collect(options.clone(), None).unwrap();
        assert_eq!(replay.sources.len(), 1);
        assert!(replay.sources[0]
            .observations
            .iter()
            .all(|message| message.durable_identity.is_some()));
        strong_observations += replay.sources[0].observations.len();
        replay_complete = replay.sources[0].backfill_complete;
        collector.commit(&replay).unwrap();
        if replay_complete {
            break;
        }
    }
    assert!(replay_complete);
    assert_eq!(strong_observations, 4);
    assert!(collector.collect(options, None).unwrap().sources.is_empty());
}

#[test]
#[serial_test::serial]
fn accounting_batch_reads_a_shared_cache_shard_once() {
    let config = tempfile::TempDir::new().unwrap();
    let home = tempfile::TempDir::new().unwrap();
    let checkpoint = tempfile::TempDir::new().unwrap();
    let mut env = crate::paths::test_env::EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", config.path());
    let sessions = home.path().join(".codex/sessions/2026/08/19");
    let identity = CacheIdentity::for_client(ClientId::Codex);
    let mut first_by_shard = std::collections::HashMap::new();
    let (first, second) = (0..1024)
        .find_map(|index| {
            let path = sessions.join(format!("session-{index}.jsonl"));
            let shard = CacheKey::new(identity, &path).shard().index;
            first_by_shard
                .insert(shard, path.clone())
                .map(|first| (first, path))
        })
        .unwrap();
    fs::create_dir_all(&sessions).unwrap();
    let mut cache = SourceMessageCache::default();
    for (index, path) in [first, second].into_iter().enumerate() {
        fs::write(&path, "{}\n").unwrap();
        let state = CodexParseState::default();
        let incremental = build_codex_incremental_cache(&path, 3, state).unwrap();
        cache.insert(CachedSourceEntry::new(
            identity,
            &path,
            SourceFingerprint::from_path(&path).unwrap(),
            vec![UnifiedMessage::new_with_dedup(
                "codex",
                "gpt-5.4",
                "openai",
                format!("session-{index}"),
                index as i64,
                TokenBreakdown::default(),
                0.0,
                Some(format!("event-{index}")),
            )],
            Vec::new(),
            Some(incremental),
        ));
    }
    cache.save_if_dirty();

    reset_shard_read_count();
    let mut collector =
        crate::accounting_delta::AccountingDeltaCollector::open_at(checkpoint.path()).unwrap();
    let delta = collector
        .collect(
            crate::accounting_delta::AccountingDeltaOptions {
                home_dir: Some(home.path().to_string_lossy().into_owned()),
                use_env_roots: false,
                ..Default::default()
            },
            None,
        )
        .unwrap();
    assert_eq!(delta.sources.len(), 2);
    assert_eq!(shard_read_count(), 1);
}
