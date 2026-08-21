use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use chrono::{TimeZone, Utc};
use tempfile::TempDir;
use toks_ingest::accounting_delta::{AccountingDeltaCollector, AccountingDeltaOptions};
use toks_ingest::bucket_tz::BucketTimezone;

use super::backend::{HistoryBackend, RefreshBatch};
use super::{HistoryStatus, LocalHistory};
use crate::history::archive;
use crate::history::{HistorySnapshot, UsageBucket, UsageSeries};

struct RealBackend {
    collector: Mutex<AccountingDeltaCollector>,
    options: AccountingDeltaOptions,
    archive: PathBuf,
    fallback: PathBuf,
}

impl RealBackend {
    fn open(root: &Path, home: &Path, fallback: &HistorySnapshot) -> Self {
        let state = root.join("collector");
        let fallback_path = root.join("last-good.json");
        fs::write(&fallback_path, serde_json::to_vec(fallback).unwrap()).unwrap();
        let scanner_settings = toks_ingest::scanner::ScannerSettings {
            bucket_timezone: Some("UTC".into()),
            ..Default::default()
        };
        Self {
            collector: Mutex::new(AccountingDeltaCollector::open_at(state).unwrap()),
            options: AccountingDeltaOptions {
                home_dir: Some(home.to_string_lossy().into_owned()),
                use_env_roots: false,
                scanner_settings,
            },
            archive: root.join("usage.sqlite3"),
            fallback: fallback_path,
        }
    }

    fn projection_batch(
        &self,
        builder: super::projected::ProjectionBuilder<'_>,
        projection: archive::ArchiveProjection,
        backlog: toks_ingest::accounting_delta::AccountingBacklog,
        made_progress: bool,
    ) -> Result<RefreshBatch> {
        let pending_sources = backlog
            .pending_sources
            .max(projection.pending_sources)
            .saturating_add(projection.projection_pending);
        Ok(RefreshBatch {
            snapshot: builder.finish(projection),
            pending_sources,
            made_progress,
        })
    }
}

impl HistoryBackend for RealBackend {
    fn hydrate_archive(&self) -> Result<Option<RefreshBatch>> {
        let Some(metadata) = archive::load_projection_metadata_at_for_test(&self.archive)? else {
            return Ok(None);
        };
        if !metadata.projection_complete {
            return Ok(None);
        }
        let timezone = BucketTimezone::from_pinned_name(Some("UTC"));
        let mut builder = super::projected::ProjectionBuilder::new(test_now(), &timezone, None);
        let projection = archive::stream_projection_at_for_test(&self.archive, |row| {
            builder.add(row);
        })?
        .ok_or_else(|| anyhow!("missing test archive projection"))?;
        let pending_sources = projection
            .pending_sources
            .saturating_add(projection.projection_pending);
        Ok(Some(RefreshBatch {
            snapshot: builder.finish(projection),
            pending_sources,
            made_progress: false,
        }))
    }

    fn load_last_good(&self) -> Option<HistorySnapshot> {
        serde_json::from_slice(&fs::read(&self.fallback).ok()?).ok()
    }

    fn refresh(&self) -> Result<RefreshBatch> {
        let mut collector = self.collector.lock().unwrap();
        let timezone = BucketTimezone::from_pinned_name(Some("UTC"));
        let mut builder = super::projected::ProjectionBuilder::new(test_now(), &timezone, None);
        let mut writer: Option<archive::ArchiveWriter> = None;
        let advance = collector
            .advance(self.options.clone(), None, |source| {
                let writer = match writer.as_mut() {
                    Some(writer) => writer,
                    None => writer.insert(archive::ArchiveWriter::open_at(
                        &self.archive,
                        1_776_210_000_000,
                    )?),
                };
                writer
                    .apply(archive::SourceDelta {
                        source_key: source.source_key.as_str(),
                        revision: source.revision.as_str(),
                        observations: source.observations,
                        backfill_complete: source.backfill_complete,
                    })
                    .map(|_| ())
            })
            .map_err(|error| anyhow!(error.to_string()))?;
        let (projection, archive_changed) = if let Some(writer) = writer {
            let applied = writer.finish(|row| builder.add(row))?;
            (applied.projection, applied.changed)
        } else {
            (
                archive::advance_projection_at_for_test(&self.archive, |row| builder.add(row))?
                    .ok_or_else(|| anyhow!("missing test archive projection"))?,
                false,
            )
        };
        self.projection_batch(
            builder,
            projection,
            advance.backlog,
            advance.archived_sources > 0 || archive_changed,
        )
    }

    fn store_last_good(&self, snapshot: &HistorySnapshot) -> Result<()> {
        fs::write(&self.fallback, serde_json::to_vec(snapshot)?)?;
        Ok(())
    }
}

fn test_now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 19, 4, 30, 0)
        .single()
        .unwrap()
}

#[test]
fn post_cutoff_hours_publish_during_backfill_and_survive_restart_exactly_once() {
    let root = TempDir::new().unwrap();
    let home = TempDir::new().unwrap();
    for index in 0..32 {
        write_session(
            home.path(),
            &format!("old-{index}"),
            "2026-08-18T12:00:00Z",
            10,
            1,
        );
    }
    write_session(home.path(), "recent-02", "2026-08-19T02:00:00Z", 20, 3);
    write_session(home.path(), "recent-03", "2026-08-19T03:00:00Z", 30, 4);
    let fallback = fallback_ending_at_one();
    let first = LocalHistory::with_backend(RealBackend::open(root.path(), home.path(), &fallback));
    assert_eq!(last_hour(&first.hydrate()), "2026-08-19 01:00");

    let refreshed = first.refresh().unwrap();
    assert!(matches!(refreshed.status, HistoryStatus::CatchingUp { .. }));
    assert_eq!(hour_tokens(&refreshed, "2026-08-19 02:00"), 21);
    assert_eq!(hour_tokens(&refreshed, "2026-08-19 03:00"), 31);
    drop(first);

    let restarted =
        LocalHistory::with_backend(RealBackend::open(root.path(), home.path(), &fallback));
    let hydrated = restarted.hydrate();
    assert_eq!(hour_tokens(&hydrated, "2026-08-19 02:00"), 21);
    assert_eq!(hour_tokens(&hydrated, "2026-08-19 03:00"), 31);
    let completed = restarted.refresh().unwrap();
    assert_eq!(hour_tokens(&completed, "2026-08-19 02:00"), 21);
    assert_eq!(hour_tokens(&completed, "2026-08-19 03:00"), 31);
}

fn fallback_ending_at_one() -> HistorySnapshot {
    HistorySnapshot {
        generated_at_ms: 1,
        usage: UsageSeries {
            hourly: vec![UsageBucket {
                key: "2026-08-19 01:00".into(),
                tokens: 1,
                ..Default::default()
            }],
            ..Default::default()
        },
        ..Default::default()
    }
}

fn last_hour(view: &super::HistoryView) -> &str {
    &view
        .snapshot
        .as_ref()
        .unwrap()
        .usage
        .hourly
        .last()
        .unwrap()
        .key
}

fn hour_tokens(view: &super::HistoryView, key: &str) -> i64 {
    view.snapshot
        .as_ref()
        .unwrap()
        .usage
        .hourly
        .iter()
        .find(|bucket| bucket.key == key)
        .unwrap()
        .tokens
}

fn write_session(home: &Path, id: &str, timestamp: &str, input: i64, modified: u64) {
    let path = home
        .join(".codex/sessions/2026/08/19")
        .join(format!("{id}.jsonl"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let contents = format!(
        concat!(
            r#"{{"timestamp":"{timestamp}","type":"session_meta","payload":{{"id":"{id}","source":"interactive","model_provider":"openai","cwd":"/repo"}}}}"#,
            "\n",
            r#"{{"timestamp":"{timestamp}","type":"turn_context","payload":{{"model":"gpt-5.4"}}}}"#,
            "\n",
            r#"{{"timestamp":"{timestamp}","type":"event_msg","payload":{{"type":"token_count","info":{{"total_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":1}},"last_token_usage":{{"input_tokens":{input},"cached_input_tokens":0,"output_tokens":1}}}}}}}}"#,
            "\n"
        ),
        timestamp = timestamp,
        id = id,
        input = input,
    );
    fs::write(&path, contents).unwrap();
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_times(fs::FileTimes::new().set_modified(UNIX_EPOCH + Duration::from_secs(modified)))
        .unwrap();
}
