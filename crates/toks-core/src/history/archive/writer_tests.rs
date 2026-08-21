use tempfile::tempdir;

use super::writer::ArchiveWriter;
use super::SourceDelta;
use toks_ingest::sessions::{CostSource, UnifiedMessage};
use toks_ingest::TokenBreakdown;

const OBSERVED_AT: i64 = 1_776_508_800_000;

#[test]
fn writer_applies_sources_sequentially_and_loads_one_projection() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let first = [message("first", 10)];
    let second = [message("second", 20)];
    let mut writer = ArchiveWriter::open_at(&path, OBSERVED_AT).unwrap();

    assert!(writer.apply(delta("first", "one", &first)).unwrap());
    assert!(writer.apply(delta("second", "one", &second)).unwrap());
    let mut total_input = 0;
    let applied = writer
        .finish(|rollup| {
            if rollup.period == super::RollupPeriod::All {
                total_input += rollup.input;
            }
        })
        .unwrap();

    assert!(applied.changed);
    assert_eq!(total_input, 30);
    assert!(applied.projection.projection_complete);
}

#[test]
fn writer_reports_an_unchanged_source_without_writes() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("usage.sqlite3");
    let observations = [message("first", 10)];
    let mut first = ArchiveWriter::open_at(&path, OBSERVED_AT).unwrap();
    first
        .apply(delta("source", "revision", &observations))
        .unwrap();
    first.finish(|_| {}).unwrap();

    let mut replay = ArchiveWriter::open_at(&path, OBSERVED_AT + 1).unwrap();
    assert!(!replay
        .apply(delta("source", "revision", &observations))
        .unwrap());
    assert!(!replay.finish(|_| {}).unwrap().changed);
}

fn delta<'a>(
    source_key: &'a str,
    revision: &'a str,
    observations: &'a [UnifiedMessage],
) -> SourceDelta<'a> {
    SourceDelta {
        source_key,
        revision,
        observations,
        backfill_complete: true,
    }
}

fn message(key: &str, input: i64) -> UnifiedMessage {
    UnifiedMessage {
        client: "codex".into(),
        provider_id: "openai".into(),
        model_id: "gpt-test".into(),
        session_id: key.into(),
        timestamp: 1_713_398_400_000 + input,
        date: String::new(),
        tokens: TokenBreakdown {
            input,
            output: 5,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        cost: 0.0,
        cost_source: CostSource::Unknown,
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: Some(key.into()),
        durable_identity: None,
        accounting_aliases: Vec::new(),
        workspace_key: None,
        workspace_label: None,
        session_title: None,
        is_turn_start: true,
        model_attribution_conflicted: false,
    }
}
