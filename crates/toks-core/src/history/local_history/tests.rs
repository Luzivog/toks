use std::fs;
use std::path::PathBuf;
use std::sync::Mutex;

use anyhow::{anyhow, Result};
use tempfile::TempDir;

use super::backend::{HistoryBackend, RefreshBatch};
use super::{carry_last_good_freshness, CatchUpRetry, HistoryStatus, LocalHistory};
use crate::history::{HistorySnapshot, UsageBucket};

struct FileBackend {
    archive: Result<Option<HistorySnapshot>, String>,
    fallback: PathBuf,
    refresh: Mutex<Result<(HistorySnapshot, usize, bool), String>>,
}

impl FileBackend {
    fn new(
        root: &TempDir,
        archive: Result<Option<HistorySnapshot>, String>,
        fallback: Option<&HistorySnapshot>,
        refresh: Result<(HistorySnapshot, usize, bool), String>,
    ) -> Self {
        let path = root.path().join("last-good.json");
        if let Some(snapshot) = fallback {
            fs::write(&path, serde_json::to_vec(snapshot).unwrap()).unwrap();
        }
        Self {
            archive,
            fallback: path,
            refresh: Mutex::new(refresh),
        }
    }
}

impl HistoryBackend for FileBackend {
    fn hydrate_archive(&self) -> Result<Option<RefreshBatch>> {
        self.archive
            .clone()
            .map(|snapshot| {
                snapshot.map(|snapshot| RefreshBatch {
                    snapshot,
                    pending_sources: 0,
                    made_progress: false,
                })
            })
            .map_err(|error| anyhow!(error))
    }

    fn load_last_good(&self) -> Option<HistorySnapshot> {
        serde_json::from_slice(&fs::read(&self.fallback).ok()?).ok()
    }

    fn refresh(&self) -> Result<RefreshBatch> {
        let outcome = self.refresh.lock().unwrap();
        match &*outcome {
            Ok((snapshot, pending_sources, made_progress)) => Ok(RefreshBatch {
                snapshot: snapshot.clone(),
                pending_sources: *pending_sources,
                made_progress: *made_progress,
            }),
            Err(error) => Err(anyhow!(error.clone())),
        }
    }

    fn store_last_good(&self, snapshot: &HistorySnapshot) -> Result<()> {
        fs::write(&self.fallback, serde_json::to_vec(snapshot)?)?;
        Ok(())
    }
}

fn snapshot(generated_at_ms: i64, hours: &[&str]) -> HistorySnapshot {
    HistorySnapshot {
        generated_at_ms,
        captured_since_ms: Some(1),
        captured_through_ms: Some(generated_at_ms),
        strong_events: hours.len() as i64,
        usage: crate::history::UsageSeries {
            hourly: hours
                .iter()
                .map(|key| UsageBucket {
                    key: (*key).into(),
                    tokens: 1,
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn recent_hours_publish_before_historical_backfill_finishes() {
    let root = TempDir::new().unwrap();
    let last_good = snapshot(100, &["2026-08-19 01:00"]);
    let recent = snapshot(
        400,
        &[
            "2026-08-19 01:00",
            "2026-08-19 02:00",
            "2026-08-19 03:00",
            "2026-08-19 04:00",
        ],
    );
    let backend = FileBackend::new(&root, Ok(None), Some(&last_good), Ok((recent, 37, true)));
    let history = LocalHistory::with_backend(backend);

    assert_eq!(
        history
            .hydrate()
            .snapshot
            .unwrap()
            .usage
            .hourly
            .last()
            .unwrap()
            .key,
        "2026-08-19 01:00"
    );
    let refreshed = history.refresh().unwrap();
    assert_eq!(
        refreshed.snapshot.as_ref().unwrap().usage.hourly[1].key,
        "2026-08-19 02:00"
    );
    assert_eq!(
        refreshed
            .snapshot
            .as_ref()
            .unwrap()
            .usage
            .hourly
            .last()
            .unwrap()
            .key,
        "2026-08-19 04:00"
    );
    assert_eq!(
        refreshed.status,
        HistoryStatus::CatchingUp {
            pending_sources: 37,
            captured_through_ms: Some(400),
            retry: CatchUpRetry::Immediate,
        }
    );
}

#[test]
fn pending_source_without_checkpoint_progress_requests_a_short_backoff() {
    let root = TempDir::new().unwrap();
    let current = snapshot(400, &["2026-08-19 04:00"]);
    let backend = FileBackend::new(
        &root,
        Ok(None),
        Some(&current),
        Ok((current.clone(), 1, false)),
    );

    let refreshed = LocalHistory::with_backend(backend).refresh().unwrap();
    assert!(matches!(
        refreshed.status,
        HistoryStatus::CatchingUp {
            retry: CatchUpRetry::ShortBackoff,
            ..
        }
    ));
}

#[test]
fn archive_hydration_wins_over_an_older_fallback() {
    let root = TempDir::new().unwrap();
    let old = snapshot(100, &["2026-08-19 01:00"]);
    let archive = snapshot(200, &["2026-08-19 01:00", "2026-08-19 02:00"]);
    let backend = FileBackend::new(&root, Ok(Some(archive)), Some(&old), Err("unused".into()));

    let hydrated = LocalHistory::with_backend(backend).hydrate();
    assert_eq!(hydrated.snapshot.unwrap().generated_at_ms, 200);
    assert_eq!(hydrated.status, HistoryStatus::Ready);
    assert!(hydrated.warning.is_none());
}

#[test]
fn failed_refresh_keeps_last_good_visible() {
    let root = TempDir::new().unwrap();
    let old = snapshot(100, &["2026-08-19 01:00"]);
    let backend = FileBackend::new(
        &root,
        Err("archive busy".into()),
        Some(&old),
        Err("writer busy".into()),
    );
    let history = LocalHistory::with_backend(backend);

    let hydrated = history.hydrate();
    assert!(matches!(
        hydrated.status,
        HistoryStatus::BusyUsingLastGood { .. }
    ));
    let refreshed = history.refresh().unwrap();
    assert_eq!(refreshed.snapshot.unwrap().generated_at_ms, 100);
    assert!(matches!(
        refreshed.status,
        HistoryStatus::BusyUsingLastGood { .. }
    ));
}

#[test]
fn active_writer_contention_is_a_nonblocking_state_even_without_history() {
    let root = TempDir::new().unwrap();
    let backend = FileBackend::new(
        &root,
        Ok(None),
        None,
        Err(toks_ingest::accounting_delta::COLLECTOR_BUSY_ERROR.into()),
    );

    let refreshed = LocalHistory::with_backend(backend).refresh().unwrap();
    assert!(refreshed.snapshot.is_none());
    assert!(matches!(
        refreshed.status,
        HistoryStatus::BusyUsingLastGood { .. }
    ));
}

#[test]
fn last_good_scan_freshness_survives_archive_hydration_when_facts_match() {
    let mut archive = snapshot(100, &["2026-08-19 01:00"]);
    archive.captured_through_ms = Some(100);
    let mut fallback = archive.clone();
    fallback.captured_through_ms = Some(200);

    carry_last_good_freshness(&mut archive, Some(&fallback));

    assert_eq!(archive.captured_through_ms, Some(200));
}
