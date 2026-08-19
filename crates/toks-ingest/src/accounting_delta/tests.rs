use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use super::{AccountingDeltaCollector, AccountingDeltaOptions};

fn options(home: &Path) -> AccountingDeltaOptions {
    AccountingDeltaOptions {
        home_dir: Some(home.to_string_lossy().into_owned()),
        use_env_roots: false,
        ..Default::default()
    }
}

fn codex_path(home: &Path) -> PathBuf {
    home.join(".codex/sessions/2026/08/19/session-a.jsonl")
}

fn session_prefix(model: &str) -> String {
    format!(
        concat!(
            r#"{{"timestamp":"2026-08-19T00:00:00Z","type":"session_meta","payload":{{"id":"session-a","source":"interactive","model_provider":"openai","cwd":"/repo"}}}}"#,
            "\n",
            r#"{{"timestamp":"2026-08-19T00:00:01Z","type":"turn_context","payload":{{"model":"{model}"}}}}"#,
            "\n"
        ),
        model = model
    )
}

fn token_line(timestamp: &str, input: i64, output: i64, last_input: i64) -> String {
    format!(
        concat!(
            r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":{output}}},"last_token_usage":{{"input_tokens":{last_input},"cached_input_tokens":0,"output_tokens":1}}}}}}}}"#,
            "\n"
        ),
        timestamp = timestamp,
        input = input,
        output = output,
        last_input = last_input
    )
}

fn model_less_token_line_with_padding(padding_bytes: usize) -> String {
    let mut line = serde_json::json!({
        "timestamp": "2026-08-19T00:00:02Z",
        "type": "event_msg",
        "payload": {
            "type": "token_count",
            "info": {
                "total_token_usage": {
                    "input_tokens": 10,
                    "cached_input_tokens": 0,
                    "output_tokens": 2
                },
                "last_token_usage": {
                    "input_tokens": 10,
                    "cached_input_tokens": 0,
                    "output_tokens": 2
                },
                "padding": "x".repeat(padding_bytes)
            }
        }
    })
    .to_string();
    line.push('\n');
    line
}

fn write_initial(home: &Path) -> PathBuf {
    let path = codex_path(home);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(
        &path,
        format!(
            "{}{}",
            session_prefix("gpt-5.4"),
            token_line("2026-08-19T00:00:02Z", 10, 2, 10)
        ),
    )
    .unwrap();
    path
}

fn setup() -> (TempDir, TempDir, AccountingDeltaCollector) {
    let home = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let collector = AccountingDeltaCollector::open_at(state.path()).unwrap();
    (home, state, collector)
}

#[test]
fn collect_does_not_advance_and_committed_unchanged_source_emits_zero() {
    let (home, _state, mut collector) = setup();
    write_initial(home.path());

    let first = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(first.sources.len(), 1);
    assert_eq!(first.sources[0].observations.len(), 1);
    let replay = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(
        replay.sources.len(),
        1,
        "collect is intentionally read-only"
    );

    collector.commit(&first).unwrap();
    let unchanged = collector.collect(options(home.path()), None).unwrap();
    assert!(unchanged.sources.is_empty());
    assert_eq!(unchanged.backlog.changed_sources, 0);
}

#[test]
fn codex_append_emits_only_the_suffix() {
    let (home, _state, mut collector) = setup();
    let path = write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    let initial_offset = first.sources[0].checkpoint.committed_offset;
    collector.commit(&first).unwrap();

    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(token_line("2026-08-19T00:00:03Z", 16, 3, 6).as_bytes())
        .unwrap();
    let appended = collector.collect(options(home.path()), None).unwrap();

    assert_eq!(appended.sources.len(), 1);
    assert_eq!(appended.sources[0].observations.len(), 1);
    assert_eq!(appended.sources[0].observations[0].tokens.input, 6);
    assert_eq!(
        appended.sources[0].checkpoint.previous_offset,
        initial_offset
    );
}

#[test]
fn incomplete_final_line_waits_for_a_complete_boundary() {
    let (home, _state, mut collector) = setup();
    let path = write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    collector.commit(&first).unwrap();
    let line = token_line("2026-08-19T00:00:03Z", 16, 3, 6);
    let split = line.len() / 2;
    let mut file = OpenOptions::new().append(true).open(&path).unwrap();
    file.write_all(&line.as_bytes()[..split]).unwrap();
    file.flush().unwrap();

    let incomplete = collector.collect(options(home.path()), None).unwrap();
    assert!(incomplete.sources.is_empty());
    assert_eq!(incomplete.backlog.pending_sources, 0);

    file.write_all(&line.as_bytes()[split..]).unwrap();
    file.flush().unwrap();
    let completed = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(completed.sources[0].observations.len(), 1);
    assert!(completed.sources[0].backfill_complete);
}

#[test]
fn bounded_range_does_not_consume_a_concurrent_append() {
    let home = TempDir::new().unwrap();
    let path = write_initial(home.path());
    let stable_end = fs::metadata(&path).unwrap().len();
    OpenOptions::new()
        .append(true)
        .open(&path)
        .unwrap()
        .write_all(token_line("2026-08-19T00:00:03Z", 16, 3, 6).as_bytes())
        .unwrap();

    let parsed = crate::sessions::codex::parse_codex_file_range(
        &path,
        0,
        stable_end,
        crate::sessions::codex::CodexParseState::default(),
    );
    assert_eq!(parsed.consumed_offset, stable_end);
    assert_eq!(parsed.messages.len(), 1);
}

#[test]
fn truncate_and_rewrite_reparses_only_that_source_from_zero() {
    let (home, _state, mut collector) = setup();
    let path = write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    let old_offset = first.sources[0].checkpoint.committed_offset;
    collector.commit(&first).unwrap();

    fs::write(
        &path,
        format!(
            "{}{}{}",
            session_prefix("gpt-5.5"),
            token_line("2026-08-19T00:00:02Z", 20, 2, 20),
            token_line("2026-08-19T00:00:03Z", 25, 3, 5)
        ),
    )
    .unwrap();
    let rewritten = collector.collect(options(home.path()), None).unwrap();

    assert_eq!(rewritten.sources.len(), 1);
    assert_eq!(rewritten.sources[0].observations.len(), 2);
    assert!(rewritten.sources[0]
        .observations
        .iter()
        .all(|message| message.model_id == "gpt-5.5"));
    assert_eq!(rewritten.sources[0].checkpoint.previous_offset, old_offset);
}

#[test]
fn parser_version_change_invalidates_only_the_source_checkpoint() {
    let (home, state, mut collector) = setup();
    write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    collector.commit(&first).unwrap();
    drop(collector);

    let state_path = state.path().join("accounting-checkpoints-v1.json");
    let mut json: serde_json::Value =
        serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
    let source = json["sources"]
        .as_object_mut()
        .unwrap()
        .values_mut()
        .next()
        .unwrap();
    source["parser_version"] = serde_json::json!(0);
    fs::write(&state_path, serde_json::to_vec(&json).unwrap()).unwrap();

    let mut reopened = AccountingDeltaCollector::open_at(state.path()).unwrap();
    let reparsed = reopened.collect(options(home.path()), None).unwrap();
    assert_eq!(reparsed.sources.len(), 1);
    assert_eq!(reparsed.sources[0].observations.len(), 1);
}

#[test]
fn codex_live_to_archive_move_keeps_the_same_source_key() {
    let home = TempDir::new().unwrap();
    let live = write_initial(home.path());
    let archived = home.path().join(".codex/archived_sessions/session-a.jsonl");
    fs::create_dir_all(archived.parent().unwrap()).unwrap();
    let key = [7_u8; 32];
    let live_key = super::identity::source_key(&key, super::types::SourceKind::Codex, &live);
    let archived_key =
        super::identity::source_key(&key, super::types::SourceKind::Codex, &archived);
    assert_eq!(live_key, archived_key);
}

#[test]
fn durable_checkpoint_contains_no_source_path() {
    let (home, state, mut collector) = setup();
    write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    collector.commit(&first).unwrap();
    let state_text =
        fs::read_to_string(state.path().join("accounting-checkpoints-v1.json")).unwrap();
    assert!(!state_text.contains(home.path().to_string_lossy().as_ref()));
    assert!(!state_text.contains("session-a.jsonl"));
    assert!(!state_text.contains("/repo"));
}

#[test]
fn empty_checkpoint_is_bounded_and_reports_the_remaining_backlog() {
    let (home, state, mut collector) = setup();
    let sessions = home.path().join(".codex/sessions/2026/08/19");
    fs::create_dir_all(&sessions).unwrap();
    for index in 0..33 {
        fs::write(
            sessions.join(format!("session-{index}.jsonl")),
            format!(
                "{}{}",
                session_prefix("gpt-5.4"),
                token_line("2026-08-19T00:00:02Z", 10, 2, 10)
            ),
        )
        .unwrap();
    }

    let first = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(first.backlog.discovered_sources, 33);
    assert_eq!(first.backlog.changed_sources, 33);
    assert_eq!(first.sources.len(), 32);
    assert_eq!(first.backlog.pending_sources, 1);
    assert!(first.backlog.scan_progress);
    let first_keys: HashSet<_> = first
        .sources
        .iter()
        .map(|source| source.source_key.as_str().to_string())
        .collect();

    drop(collector);
    let mut reopened = AccountingDeltaCollector::open_at(state.path()).unwrap();
    let second = reopened.collect(options(home.path()), None).unwrap();
    assert!(second
        .sources
        .iter()
        .any(|source| !first_keys.contains(source.source_key.as_str())));
}

#[test]
fn codex_semantic_context_beyond_a_four_megabyte_record_makes_progress() {
    let (home, _state, mut collector) = setup();
    let path = codex_path(home.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let session_meta = concat!(
        r#"{"timestamp":"2026-08-19T00:00:00Z","type":"session_meta","payload":{"id":"session-a","source":"interactive","model_provider":"openai"}}"#,
        "\n"
    );
    let model = concat!(
        r#"{"timestamp":"2026-08-19T00:00:03Z","type":"turn_context","payload":{"model":"gpt-5.4"}}"#,
        "\n"
    );
    fs::write(
        &path,
        format!(
            "{session_meta}{}{model}",
            model_less_token_line_with_padding(4 * 1024 * 1024)
        ),
    )
    .unwrap();

    let delta = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(delta.sources.len(), 1);
    assert_eq!(delta.sources[0].observations.len(), 1);
    assert_eq!(delta.sources[0].observations[0].model_id, "gpt-5.4");
    assert!(delta.sources[0].backfill_complete);
    collector.commit(&delta).unwrap();
    assert!(collector
        .collect(options(home.path()), None)
        .unwrap()
        .sources
        .is_empty());
}

#[test]
fn malformed_codex_line_is_skipped_without_wedging_the_valid_suffix() {
    let (home, _state, mut collector) = setup();
    let path = write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    collector.commit(&first).unwrap();
    let mut file = OpenOptions::new().append(true).open(path).unwrap();
    file.write_all(b"not-json\n").unwrap();
    file.write_all(token_line("2026-08-19T00:00:03Z", 16, 3, 6).as_bytes())
        .unwrap();
    file.flush().unwrap();

    let appended = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(appended.sources.len(), 1);
    assert_eq!(appended.sources[0].observations.len(), 1);
    assert_eq!(appended.sources[0].observations[0].tokens.input, 6);
    collector.commit(&appended).unwrap();
    assert!(collector
        .collect(options(home.path()), None)
        .unwrap()
        .sources
        .is_empty());
}

#[test]
fn permanently_malformed_codex_source_advances_once() {
    let (home, _state, mut collector) = setup();
    let path = codex_path(home.path());
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, "not-json\n").unwrap();

    let malformed = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(malformed.sources.len(), 1);
    assert!(malformed.sources[0].observations.is_empty());
    assert!(malformed.sources[0].backfill_complete);
    collector.commit(&malformed).unwrap();
    assert!(collector
        .collect(options(home.path()), None)
        .unwrap()
        .sources
        .is_empty());
}

#[test]
#[serial_test::serial]
fn source_message_cache_seed_is_archived_then_suffix_is_exact_once() {
    let home = TempDir::new().unwrap();
    let state = TempDir::new().unwrap();
    let config = TempDir::new().unwrap();
    let mut env = crate::paths::test_env::EnvGuard::capture(&["TOKSCOPE_CONFIG_DIR"]);
    env.set("TOKSCOPE_CONFIG_DIR", config.path());
    let path = write_initial(home.path());
    let home_text = home.path().to_string_lossy();
    let cached =
        crate::parse_all_messages_with_pricing(home_text.as_ref(), &["codex".to_string()], None);
    assert_eq!(cached.len(), 1);
    OpenOptions::new()
        .append(true)
        .open(path)
        .unwrap()
        .write_all(token_line("2026-08-19T00:00:03Z", 16, 3, 6).as_bytes())
        .unwrap();

    let mut collector = AccountingDeltaCollector::open_at(state.path()).unwrap();
    let seeded = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(seeded.sources.len(), 1);
    assert_eq!(seeded.sources[0].observations.len(), 2);
    assert_eq!(seeded.sources[0].observations[0].tokens.input, 10);
    assert_eq!(seeded.sources[0].observations[1].tokens.input, 6);
    collector.commit(&seeded).unwrap();
    assert!(collector
        .collect(options(home.path()), None)
        .unwrap()
        .sources
        .is_empty());
}

#[test]
fn checkpoint_writer_lock_is_nonblocking_and_released_on_drop() {
    let state = TempDir::new().unwrap();
    let first = AccountingDeltaCollector::open_at(state.path()).unwrap();
    let error = match AccountingDeltaCollector::open_at(state.path()) {
        Ok(_) => panic!("a second checkpoint writer unexpectedly acquired the lock"),
        Err(error) => error,
    };
    assert_eq!(error, super::COLLECTOR_BUSY_ERROR);

    drop(first);
    AccountingDeltaCollector::open_at(state.path()).unwrap();
}

#[test]
fn bounded_samples_detect_same_size_same_mtime_rewrite() {
    let (home, _state, mut collector) = setup();
    let path = write_initial(home.path());
    let first = collector.collect(options(home.path()), None).unwrap();
    collector.commit(&first).unwrap();
    let original_modified = fs::metadata(&path).unwrap().modified().unwrap();
    let replacement = format!(
        "{}{}",
        session_prefix("gpt-5.5"),
        token_line("2026-08-19T00:00:02Z", 10, 2, 10)
    );
    assert_eq!(replacement.len() as u64, fs::metadata(&path).unwrap().len());
    fs::write(&path, replacement).unwrap();
    let file = OpenOptions::new().write(true).open(&path).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(original_modified))
        .unwrap();

    let rewritten = collector.collect(options(home.path()), None).unwrap();
    assert_eq!(rewritten.sources.len(), 1);
    assert!(rewritten.sources[0]
        .observations
        .iter()
        .all(|message| message.model_id == "gpt-5.5"));
}

#[test]
fn identical_names_under_managed_roots_have_distinct_source_keys() {
    let temporary = TempDir::new().unwrap();
    let first = temporary.path().join("account-a/sessions/session.jsonl");
    let second = temporary.path().join("account-b/sessions/session.jsonl");
    let key = [9_u8; 32];
    let first_key = super::identity::source_key(&key, super::types::SourceKind::Codex, &first);
    let second_key = super::identity::source_key(&key, super::types::SourceKind::Codex, &second);
    assert_ne!(first_key, second_key);
}
