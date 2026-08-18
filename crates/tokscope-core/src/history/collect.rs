use anyhow::{anyhow, Result};
use chrono::Utc;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokscope_ingest::bucket_tz::BucketTimezone;
use tokscope_ingest::pricing::PricingService;
use tokscope_ingest::{parse_local_unified_messages_with_pricing, LocalParseOptions};

use super::rollup::merge_usage_series;
use super::source::Accum;
use super::{CostCoverage, HistorySnapshot, SourceHistory, UsageKey, UsagePeriod, CLIENTS};

pub fn collect() -> Result<HistorySnapshot> {
    let snapshot = collect_live()?;
    super::validation::validate(&snapshot)?;
    // Storage failure must not hide a fresh, valid aggregate. The cache write
    // itself is atomic, so the previous last-good snapshot remains intact.
    let _ = super::cache::store(&snapshot);
    Ok(snapshot)
}

fn collect_live() -> Result<HistorySnapshot> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| anyhow!("tokio runtime: {e}"))?;

    // Pricing starts from Tokscope's embedded baseline/local cache; optional
    // catalog refresh happens behind the ingest service without blocking this
    // first aggregate.
    let svc: Option<Arc<PricingService>> = match rt.block_on(PricingService::get_or_init()) {
        Ok(svc) => Some(svc),
        Err(_) => PricingService::load_cached_any_age().map(Arc::new),
    };

    let mut extra_scan_paths: BTreeMap<String, Vec<std::path::PathBuf>> = BTreeMap::new();
    for profile in crate::accounts::discover_profiles()
        .into_iter()
        .filter(|profile| profile.managed)
    {
        let roots = extra_scan_paths
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
    let scanner_settings = tokscope_ingest::scanner::ScannerSettings {
        extra_scan_paths,
        ..Default::default()
    };
    let bucket_timezone = BucketTimezone::from_scanner_settings(&scanner_settings);
    let options = LocalParseOptions {
        home_dir: None,
        use_env_roots: true,
        clients: Some(CLIENTS.iter().map(|s| s.to_string()).collect()),
        since: None,
        until: None,
        year: None,
        scanner_settings,
    };
    let messages = rt
        .block_on(parse_local_unified_messages_with_pricing(
            options,
            svc.as_deref(),
        ))
        .map_err(|e| anyhow!("Tokscope scan failed: {e}"))?;

    let now = Utc::now();
    let now_minute = now.timestamp() / 60;
    let today_key = bucket_timezone.day_key(now.timestamp_millis());
    let today = match UsageKey::parse(UsagePeriod::Daily, &today_key) {
        Some(UsageKey::Daily(date)) => date,
        _ => now.with_timezone(&chrono::Local).date_naive(),
    };

    let mut per_client: HashMap<&str, Accum> = HashMap::new();
    for m in &messages {
        let client: &str = match m.client.as_str() {
            "claude" => "claude",
            "codex" => "codex",
            _ => continue,
        };
        per_client
            .entry(client)
            .or_default()
            .add(m, now_minute, &bucket_timezone);
    }

    let mut sources: Vec<SourceHistory> = CLIENTS
        .iter()
        .filter_map(|c| {
            per_client
                .remove(*c)
                .map(|a| a.finish(c, now_minute, today))
        })
        .collect();
    sources.sort_by(|a, b| a.client.cmp(&b.client));
    let usage = merge_usage_series(&sources);
    let mut cost_coverage = CostCoverage::default();
    for source in &sources {
        cost_coverage.add_assign(source.cost_coverage);
    }

    Ok(HistorySnapshot {
        sources,
        usage,
        generated_at_ms: now.timestamp_millis(),
        cost_coverage,
        unpriced: !cost_coverage.is_complete(),
    })
}
