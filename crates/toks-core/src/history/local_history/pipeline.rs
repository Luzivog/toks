use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use chrono::Utc;
use toks_ingest::accounting_delta::{AccountingDeltaCollector, AccountingDeltaOptions};
use toks_ingest::bucket_tz::BucketTimezone;
use toks_ingest::pricing::PricingService;

use super::super::{archive, HistorySnapshot};
use super::backend::RefreshBatch;

pub(super) fn refresh(collector: &mut AccountingDeltaCollector) -> Result<RefreshBatch> {
    super::super::cache::preserve_legacy_snapshot();
    let scanner_settings = scanner_settings();
    if let Some(batch) = migrate_projection_before_ingest(&scanner_settings)? {
        return Ok(batch);
    }
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| anyhow!("tokio runtime: {error}"))?;
    let pricing: Option<Arc<PricingService>> = match runtime.block_on(PricingService::get_or_init())
    {
        Ok(service) => Some(service),
        Err(_) => PricingService::load_cached_any_age().map(Arc::new),
    };
    let timezone = BucketTimezone::from_scanner_settings(&scanner_settings);
    let options = AccountingDeltaOptions {
        home_dir: None,
        use_env_roots: true,
        scanner_settings,
    };
    let observed_at_ms = Utc::now().timestamp_millis();
    let now = Utc::now();
    let mut projection_builder =
        super::projected::ProjectionBuilder::new(now, &timezone, pricing.as_deref());
    let mut writer: Option<archive::ArchiveWriter> = None;
    let advance = collector
        .advance(options, pricing.as_deref(), |source| {
            let writer = match writer.as_mut() {
                Some(writer) => writer,
                None => writer.insert(archive::ArchiveWriter::open_default(observed_at_ms)?),
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
        let applied = writer.finish(|row| projection_builder.add(row))?;
        (Some(applied.projection), applied.changed)
    } else {
        (
            archive::refresh_projection_default(observed_at_ms, |row| {
                projection_builder.add(row);
            })?,
            false,
        )
    };
    let projection = match projection {
        Some(projection) => projection,
        None => return fallback_or_empty(advance.backlog.pending_sources),
    };
    let pending_sources = advance
        .backlog
        .pending_sources
        .max(projection.pending_sources)
        .saturating_add(projection_backlog(&projection));
    let snapshot = projection_builder.finish(projection);
    Ok(RefreshBatch {
        snapshot,
        pending_sources,
        // Moving the fair-scan cursor is not durable accounting progress. If
        // every selected source is still incomplete, back off before polling
        // again instead of spinning across the same live files.
        made_progress: advance.archived_sources > 0 || archive_changed,
    })
}

pub(super) fn hydrate_archive() -> Result<Option<RefreshBatch>> {
    let Some(metadata) = archive::load_metadata_default()? else {
        return Ok(None);
    };
    // A newly created compact projection can be empty while the legacy v2
    // archive is still migrating. Keep the last-good snapshot on startup;
    // bounded refresh steps advance migration and publish recent committed data.
    if !metadata.projection_complete {
        return Ok(None);
    }
    let settings = toks_ingest::scanner::ScannerSettings::default();
    let timezone = BucketTimezone::from_scanner_settings(&settings);
    let pricing = PricingService::load_cached_any_age();
    let mut builder =
        super::projected::ProjectionBuilder::new(Utc::now(), &timezone, pricing.as_ref());
    let projection = archive::load_default(|row| builder.add(row))?
        .ok_or_else(|| anyhow!("usage archive disappeared during hydration"))?;
    let pending_sources = projection
        .pending_sources
        .saturating_add(projection_backlog(&projection));
    let snapshot = builder.finish(projection);
    super::super::validation::validate(&snapshot)?;
    Ok(Some(RefreshBatch {
        snapshot,
        pending_sources,
        made_progress: false,
    }))
}

fn fallback_or_empty(pending_sources: usize) -> Result<RefreshBatch> {
    let snapshot = super::super::cache::load().unwrap_or_else(|| HistorySnapshot {
        generated_at_ms: Utc::now().timestamp_millis(),
        ..Default::default()
    });
    Ok(RefreshBatch {
        snapshot,
        pending_sources,
        made_progress: false,
    })
}

fn migrate_projection_before_ingest(
    settings: &toks_ingest::scanner::ScannerSettings,
) -> Result<Option<RefreshBatch>> {
    let Some(before) = archive::load_metadata_default()? else {
        return Ok(None);
    };
    if before.projection_complete {
        return Ok(None);
    }
    let before_pending = before.projection_pending;
    let observed_at_ms = Utc::now().timestamp_millis();
    let timezone = BucketTimezone::from_scanner_settings(settings);
    let pricing = PricingService::load_cached_any_age();
    let mut builder =
        super::projected::ProjectionBuilder::new(Utc::now(), &timezone, pricing.as_ref());
    let Some(projection) = archive::refresh_projection_default(observed_at_ms, |row| {
        builder.add(row);
    })?
    else {
        return Ok(None);
    };
    let made_progress =
        projection.projection_complete || projection.projection_pending < before_pending;
    // Completing an upgrade still needs one source-discovery step. Force that
    // next step without parsing a 32-source batch before the archive is visible.
    let pending_sources = projection
        .pending_sources
        .saturating_add(projection_backlog(&projection))
        .max(usize::from(projection.projection_complete));
    Ok(Some(RefreshBatch {
        snapshot: builder.finish(projection),
        pending_sources,
        made_progress,
    }))
}

fn projection_backlog(projection: &archive::ArchiveProjection) -> usize {
    if projection.projection_complete {
        0
    } else {
        projection.projection_pending.max(1)
    }
}

fn scanner_settings() -> toks_ingest::scanner::ScannerSettings {
    let mut paths: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    for profile in crate::accounts::discover_profiles()
        .into_iter()
        .filter(|profile| profile.managed)
    {
        let roots = paths
            .entry(profile.provider.slug().to_string())
            .or_default();
        match profile.provider {
            crate::Provider::Claude => {
                roots.push(profile.config_dir.join("projects"));
                roots.push(profile.config_dir.join("transcripts"));
            }
            crate::Provider::Codex => {
                roots.push(profile.config_dir.join("sessions"));
                roots.push(profile.config_dir.join("archived_sessions"));
            }
        }
    }
    toks_ingest::scanner::ScannerSettings {
        extra_scan_paths: paths,
        ..Default::default()
    }
}
