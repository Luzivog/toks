//! GitHub Copilot Desktop SQLite parser.
//!
//! The macOS desktop app stores aggregate token totals in `~/.copilot/data.db`
//! and per-session event metadata in `~/.copilot/session-state/{session_id}`.

use super::utils::lossy_lines;
use super::{normalize_workspace_key, workspace_label_from_key, UnifiedMessage};
use crate::provider_identity::inferred_provider_from_model;
use chrono::{DateTime, NaiveDateTime};
use rusqlite::{Connection, OpenFlags};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{HashMap, HashSet};
use std::io::BufReader;
use std::path::Path;
use tracing::warn;

#[derive(Debug)]
struct CopilotDesktopSessionRow {
    id: String,
    model: Option<String>,
    total_input_tokens: i64,
    total_output_tokens: i64,
    total_cached_tokens: i64,
    total_reasoning_tokens: i64,
    created_at: Option<String>,
}

#[derive(Debug, Default)]
struct SessionStateMetadata {
    model: Option<String>,
    cwd: Option<String>,
    shutdowns: Vec<ShutdownUsage>,
    /// Usage the shutdown snapshots account for. This is normally the sum of
    /// `shutdowns`, but not when a snapshot was swallowed as an unknown
    /// baseline: that usage is accounted for without being emitted, and the
    /// row residual has to know it so it is not re-emitted on `created_at`.
    consumed: UsageBuckets,
}

/// One model's usage from a single `session.shutdown` record.
///
/// These carry their own timestamp, which is the only per-run timing the
/// desktop app exposes: the `sessions` row has a lifetime total and an
/// immutable `created_at`.
///
/// As read off disk the numbers are **cumulative**, not per-run: the Copilot
/// SDK's `UsageMetricsTracker` only ever adds to its per-model counters and
/// exposes no reset, and `Session.shutdown()` emits whatever the tracker holds
/// at that moment with no one-shot guard. So a session that shuts down twice
/// writes two snapshots of the same running total. [`shutdown_deltas`] turns
/// them into the per-run increments the rest of this module assumes.
#[derive(Debug, Clone)]
struct ShutdownUsage {
    /// Identity of the originating event, used to build a dedup key that
    /// survives rotation or compaction of `events.jsonl`. Position in the file
    /// would not: dropping one earlier line renumbers every record after it,
    /// so already-submitted rows would come back under new keys and be counted
    /// twice. The event's own `id` is a UUID; its `timestamp` is the fallback.
    event_id: String,
    timestamp_ms: i64,
    /// The `modelMetrics` key, trimmed. That key is the identity of the
    /// tracker counter these numbers came from, so it is what the running peak,
    /// the verbatim-record dedup, and the submitted dedup key are all grouped
    /// by. Trimming it here keeps that grouping identical to the model the
    /// emitted message is attributed to, which is trimmed too: two spellings
    /// that differ only by padding are one model everywhere or nowhere.
    model: String,
    /// Concrete model active for this shutdown when the tracker key is `auto`.
    attributed_model: Option<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
}

/// The five token buckets a shutdown record reports, in a fixed order so
/// cumulative snapshots can be differenced bucket-by-bucket.
type UsageBuckets = [i64; 5];

impl ShutdownUsage {
    fn buckets(&self) -> UsageBuckets {
        [
            self.input,
            self.output,
            self.cache_read,
            self.cache_write,
            self.reasoning,
        ]
    }

    fn with_buckets(self, buckets: UsageBuckets) -> Self {
        Self {
            input: buckets[0],
            output: buckets[1],
            cache_read: buckets[2],
            cache_write: buckets[3],
            reasoning: buckets[4],
            ..self
        }
    }
}

/// What one session's shutdown snapshots resolve to: the increments to emit,
/// and the usage they account for.
struct ShutdownAttribution {
    /// One entry per snapshot that added usage, already differenced into the
    /// increment it contributed.
    deltas: Vec<ShutdownUsage>,
    /// The total the snapshots account for, which the caller subtracts from the
    /// row's lifetime total. Not necessarily the sum of `deltas`.
    consumed: UsageBuckets,
}

/// Convert cumulative shutdown snapshots into the usage each one actually
/// added, so summing them reconciles against the row's lifetime total instead
/// of multiplying it.
///
/// Without this, a session that emitted an error shutdown at 100 tokens and a
/// routine one at 200 contributes 300 — the earlier snapshot counted twice,
/// and spread across two different days.
///
/// Snapshots are grouped by their `modelMetrics` key, which is the identity of
/// the tracker counter they were read from and the same identity the emitted
/// message is keyed and attributed by.
///
/// A snapshot that reports *less* than one before it (the tracker restarted
/// with the session, or the records arrived out of order) contributes nothing
/// rather than a negative bucket: each model's running peak is the baseline.
/// Cache-read growth is additionally capped by inclusive-input growth in the
/// same snapshot, because input includes cache reads and the two cannot safely
/// advance independently. Anything the snapshots leave unexplained is still
/// reconciled by the caller's residual against the `sessions` row.
///
/// `complete_from_start` says whether the log still begins where the session
/// did. When it does, the first snapshot seen for a model really is that
/// model's first, and zero is the right baseline to difference it from. When
/// it does not, an earlier snapshot may have been rotated away: that snapshot's
/// increment was already submitted under a dedup key this parse can no longer
/// reproduce, so differencing the survivor from zero would re-emit it under a
/// key whose day is only ever ratcheted upwards. The survivor is treated as an
/// unknown baseline instead — it sets the peak and contributes nothing.
///
/// That is deliberately the conservative direction. On a machine that had
/// already submitted the rotated-away snapshot it is exact; on one scanning a
/// truncated log for the first time it under-reports the baseline rather than
/// re-dating it, because nothing on disk distinguishes the two cases.
fn shutdown_deltas(
    mut snapshots: Vec<ShutdownUsage>,
    complete_from_start: bool,
) -> ShutdownAttribution {
    // A record repeated verbatim — the same event written twice by a
    // re-flushed or replayed log — describes one shutdown and must count once.
    let mut seen = HashSet::new();
    snapshots.retain(|snapshot| seen.insert((snapshot.event_id.clone(), snapshot.model.clone())));

    // Order by the envelope timestamp so "previous snapshot" means what it
    // says; the sort is stable, so records sharing a timestamp keep file order.
    snapshots.sort_by_key(|snapshot| snapshot.timestamp_ms);

    let mut peaks: HashMap<String, UsageBuckets> = HashMap::new();
    let mut deltas = Vec::with_capacity(snapshots.len());
    for snapshot in snapshots {
        let current = snapshot.buckets();
        let baseline = match peaks.get(&snapshot.model) {
            Some(peak) => Some(*peak),
            None if complete_from_start => Some(UsageBuckets::default()),
            None => None,
        };

        let peak = peaks.entry(snapshot.model.clone()).or_insert(current);
        for (index, value) in current.iter().enumerate() {
            peak[index] = peak[index].max(*value);
        }

        let Some(baseline) = baseline else {
            continue;
        };
        let mut delta = UsageBuckets::default();
        for (index, value) in current.iter().enumerate() {
            delta[index] = value.saturating_sub(baseline[index]).max(0);
        }
        // `inputTokens` includes cache reads. If a reset/out-of-order snapshot
        // lowers inclusive input while raising cache reads, emitting that cache
        // growth independently would mint tokens: normalization would subtract
        // it from a zero input delta and then retain it as a cache bucket. Any
        // cache-read increment is therefore bounded by the inclusive-input
        // increment observed in the same snapshot.
        delta[2] = delta[2].min(delta[0]);
        if delta.iter().all(|bucket| *bucket == 0) {
            continue;
        }
        deltas.push(snapshot.with_buckets(delta));
    }

    // A later reset can reveal a higher cache-read composition without any
    // inclusive-input growth. The cap above prevents minting tokens, then this
    // pass uses spare cache capacity in already-authorized input increments so
    // the emitted bucket totals still match the final cache high-water. Newest
    // authorized increments receive the reclassification first; no increment
    // can hold more cache reads than inclusive input.
    for (model, peak) in &peaks {
        let target_cache = peak[2].min(peak[0]);
        let assigned_cache: i64 = deltas
            .iter()
            .filter(|delta| delta.model == *model)
            .map(|delta| delta.cache_read)
            .sum();
        let mut remaining_cache = target_cache.saturating_sub(assigned_cache);
        for delta in deltas
            .iter_mut()
            .rev()
            .filter(|delta| delta.model == *model)
        {
            let capacity = delta.input.saturating_sub(delta.cache_read);
            let reassigned = capacity.min(remaining_cache);
            delta.cache_read = delta.cache_read.saturating_add(reassigned);
            remaining_cache = remaining_cache.saturating_sub(reassigned);
            if remaining_cache == 0 {
                break;
            }
        }
    }

    // Every model's peak is the highest total it was ever observed holding, so
    // summing the peaks is what the snapshots account for whether or not each
    // one was emitted. Using the emitted deltas instead would hand a swallowed
    // baseline back to the residual and re-date it to `created_at`.
    let consumed = peaks
        .values()
        .fold(UsageBuckets::default(), |mut total, peak| {
            for (index, value) in peak.iter().enumerate() {
                total[index] = total[index].saturating_add(*value);
            }
            total
        });

    ShutdownAttribution { deltas, consumed }
}

pub fn parse_copilot_desktop_db(db_path: &Path) -> Vec<UnifiedMessage> {
    let conn = match Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(conn) => conn,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to open Copilot Desktop database"
            );
            return Vec::new();
        }
    };

    let mut stmt = match conn.prepare(
        r#"
        SELECT
            id,
            title,
            model,
            total_input_tokens,
            total_output_tokens,
            total_cached_tokens,
            total_reasoning_tokens,
            total_nano_aiu,
            created_at
        FROM sessions
        WHERE total_input_tokens > 0
           OR total_output_tokens > 0
           OR total_cached_tokens > 0
           OR total_reasoning_tokens > 0
        "#,
    ) {
        Ok(stmt) => stmt,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to prepare Copilot Desktop sessions query"
            );
            return Vec::new();
        }
    };

    let rows = match stmt.query_map([], |row| {
        Ok(CopilotDesktopSessionRow {
            id: row.get(0)?,
            model: row.get(2)?,
            total_input_tokens: row.get::<_, Option<i64>>(3)?.unwrap_or(0),
            total_output_tokens: row.get::<_, Option<i64>>(4)?.unwrap_or(0),
            total_cached_tokens: row.get::<_, Option<i64>>(5)?.unwrap_or(0),
            total_reasoning_tokens: row.get::<_, Option<i64>>(6)?.unwrap_or(0),
            created_at: row.get(8)?,
        })
    }) {
        Ok(rows) => rows,
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to execute Copilot Desktop sessions query"
            );
            return Vec::new();
        }
    };

    rows.flat_map(|row| match row {
        Ok(row) => session_row_to_messages(db_path, row),
        Err(err) => {
            warn!(
                db_path = %db_path.display(),
                error = %err,
                "Failed to decode Copilot Desktop session row"
            );
            Vec::new()
        }
    })
    .collect()
}

/// Turn one `sessions` row into the messages its usage actually belongs to.
///
/// The row holds a lifetime total against an immutable `created_at`, so
/// emitting it as-is re-dated every later turn to the day the session was
/// opened: that day grew on every rescan and the days the tokens were really
/// spent on received none of them (#962).
///
/// `session.shutdown` records carry their own timestamp and a per-model
/// breakdown, so each one is emitted at its own time and under its own model.
/// Their token counts are cumulative, so [`shutdown_deltas`] has already
/// reduced them to per-run increments by the time they arrive here.
/// Whatever they do not account for — a run that died before writing its
/// shutdown, or a session recorded by the CLI rather than the desktop app —
/// stays on `created_at` under the row's original dedup key, so the row
/// remains the authority on the all-time total and nothing is dropped.
fn session_row_to_messages(db_path: &Path, row: CopilotDesktopSessionRow) -> Vec<UnifiedMessage> {
    let metadata = read_session_state_metadata(db_path, &row.id);
    let fallback_model = metadata
        .model
        .as_deref()
        .or(row.model.as_deref())
        .map(str::trim)
        .filter(|model| !model.is_empty())
        .unwrap_or("auto")
        .to_string();

    let created_at_ms = row
        .created_at
        .as_deref()
        .and_then(parse_iso8601_timestamp_ms)
        .unwrap_or_else(|| {
            warn!(
                session_id = %row.id,
                created_at = ?row.created_at,
                "Copilot Desktop session has unparseable created_at; defaulting to 0"
            );
            0
        });

    let workspace_key = metadata.cwd.as_deref().and_then(normalize_workspace_key);
    let build = |model_id: String, timestamp_ms: i64, tokens, dedup_key: String| {
        let provider_id = inferred_provider_from_model(&model_id)
            .unwrap_or("github-copilot")
            .to_string();
        let mut message = UnifiedMessage::new_with_dedup(
            "copilot",
            model_id,
            provider_id,
            row.id.clone(),
            timestamp_ms,
            tokens,
            0.0,
            Some(dedup_key),
        );
        if let Some(workspace_key) = workspace_key.clone() {
            let workspace_label = workspace_label_from_key(&workspace_key);
            message.set_workspace(Some(workspace_key), workspace_label);
        }
        message
    };

    let mut messages = Vec::with_capacity(metadata.shutdowns.len() + 1);
    // The SQLite row is authoritative for every bucket it stores. The sidecar
    // and DB are separate files, so a shutdown can be observed before the row
    // catches up; consume a per-row budget to prevent that race from emitting
    // more lifetime usage than the row. Cache-write has no SQLite column and
    // remains sidecar-authoritative.
    let mut remaining_input = row.total_input_tokens.max(0);
    let mut remaining_output = row.total_output_tokens.max(0);
    let mut remaining_cache_read = row.total_cached_tokens.max(0);
    let mut remaining_reasoning = row.total_reasoning_tokens.max(0);
    for shutdown in &metadata.shutdowns {
        // `auto` is resolved for display and pricing only. It is a tracker
        // counter of its own — `modelMetrics` is keyed by the model each
        // `assistant.usage` event reported — so it keeps its own peak and its
        // own dedup key even when it is attributed to the resolved model.
        // Folding it into `fallback_model` before differencing would subtract
        // one counter's peak from another counter's total.
        let model_id = shutdown
            .attributed_model
            .clone()
            .unwrap_or_else(|| fallback_model.clone());
        let input = shutdown.input.min(remaining_input);
        let output = shutdown.output.min(remaining_output);
        let cache_read = shutdown.cache_read.min(remaining_cache_read).min(input);
        let reasoning = shutdown.reasoning.min(remaining_reasoning);
        remaining_input = remaining_input.saturating_sub(input);
        remaining_output = remaining_output.saturating_sub(output);
        remaining_cache_read = remaining_cache_read.saturating_sub(cache_read);
        remaining_reasoning = remaining_reasoning.saturating_sub(reasoning);
        let tokens = super::copilot::normalize_input_tokens(
            input,
            output,
            cache_read,
            shutdown.cache_write,
            reasoning,
        );
        if tokens.total() == 0 {
            continue;
        }
        messages.push(build(
            model_id,
            shutdown.timestamp_ms,
            tokens,
            format!(
                "copilot-desktop:{}:shutdown:{}:{}",
                row.id, shutdown.event_id, shutdown.model
            ),
        ));
    }

    // What the snapshots account for, which is not always what they emitted.
    // An unknown baseline may already have been submitted before the log head
    // disappeared, so re-emitting it would inflate that machine permanently.
    // On a first-ever scan of an already-truncated log this deliberately
    // under-reports instead; no remaining record can distinguish those cases
    // (see shutdown_deltas' safety tradeoff above).
    let consumed = metadata.consumed;
    // The row's own cache-write column does not exist, so the shutdown records
    // are the only source for that bucket and there is nothing to reconcile.
    let residual_input = (row.total_input_tokens - consumed[0]).max(0);
    let residual_cache_read = (row.total_cached_tokens - consumed[2])
        .max(0)
        .min(residual_input);
    let residual = super::copilot::normalize_input_tokens(
        residual_input,
        (row.total_output_tokens - consumed[1]).max(0),
        residual_cache_read,
        0,
        (row.total_reasoning_tokens - consumed[4]).max(0),
    );

    if residual.total() > 0 {
        messages.push(build(
            fallback_model,
            created_at_ms,
            residual,
            format!("copilot-desktop:{}", row.id),
        ));
    }

    // The SQLite row has always represented one Copilot session/message. The
    // shutdown metadata only splits that row by time and model; it must not
    // turn one legacy count into one count per attributed fragment. Assign the
    // authoritative count to exactly one fragment and make every other split
    // row count-neutral.
    for (index, message) in messages.iter_mut().enumerate() {
        message.message_count = i32::from(index == 0);
    }

    messages
}

fn read_session_state_metadata(db_path: &Path, session_id: &str) -> SessionStateMetadata {
    let Some(copilot_root) = db_path.parent() else {
        return SessionStateMetadata::default();
    };
    let events_path = copilot_root
        .join("session-state")
        .join(session_id)
        .join("events.jsonl");

    read_events_metadata(&events_path)
}

fn read_events_metadata(events_path: &Path) -> SessionStateMetadata {
    let file = match std::fs::File::open(events_path) {
        Ok(file) => file,
        Err(_) => return SessionStateMetadata::default(),
    };

    let mut metadata = SessionStateMetadata::default();
    // The SDK builds a session by replaying this file and rejects one whose
    // first event is not `session.start`, and the only removal it performs
    // keeps the prefix and drops the tail. A log that does not open with
    // `session.start` has therefore lost its head to something else, and the
    // records that used to precede the survivors are unrecoverable.
    let mut first_event_type: Option<String> = None;
    for line in lossy_lines(BufReader::new(file)) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Ok(event) = serde_json::from_str::<Value>(trimmed) else {
            continue;
        };
        let Some(event_type) = event.get("type").and_then(Value::as_str) else {
            continue;
        };
        if first_event_type.is_none() {
            first_event_type = Some(event_type.to_string());
        }

        match event_type {
            "session.start" if metadata.cwd.is_none() => {
                metadata.cwd = event
                    .pointer("/data/context/cwd")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|cwd| !cwd.is_empty())
                    .map(str::to_string);
            }
            "session.model_change" => {
                if let Some(model) = event
                    .pointer("/data/newModel")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|model| !model.is_empty() && model != &"auto")
                {
                    metadata.model = Some(model.to_string());
                }
            }
            "session.shutdown" => collect_shutdown_usage(&event, &mut metadata.shutdowns),
            _ => {}
        }
    }

    // Everything downstream treats one entry as one run's spend, so hand back
    // increments rather than the cumulative snapshots the app writes.
    let complete_from_start = first_event_type.as_deref() == Some("session.start");
    let attribution = shutdown_deltas(std::mem::take(&mut metadata.shutdowns), complete_from_start);
    metadata.shutdowns = attribution.deltas;
    metadata.consumed = attribution.consumed;
    metadata
}

fn collect_shutdown_usage(event: &Value, out: &mut Vec<ShutdownUsage>) {
    // The desktop app nests event payloads under `data`; a flat record is
    // accepted too so a shutdown that omits the envelope still reports usage
    // rather than silently contributing nothing.
    let payload = event.get("data").unwrap_or(event);
    // The timestamp lives on the envelope next to `id`/`parentId`, not in the
    // payload, and it is an ISO-8601 string. Reading the payload first only
    // matters for a flat record that has no envelope to read from.
    let Some(timestamp_ms) = event
        .get("timestamp")
        .or_else(|| payload.get("timestamp"))
        .and_then(Value::as_str)
        .and_then(parse_iso8601_timestamp_ms)
    else {
        return;
    };
    // `events.jsonl` is append-only in practice, but nothing guarantees it
    // stays that way, so key off the event's own identity rather than its
    // position in the file.
    let event_id = event
        .get("id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| {
            // Real records carry UUIDs. For a malformed/legacy record without
            // one, hash the stable event content: a timestamp alone collides
            // when two distinct shutdowns share the same millisecond, while a
            // file ordinal would change when earlier lines rotate away.
            let digest = Sha256::digest(event.to_string().as_bytes());
            format!("anon-{digest:x}")
        });
    let Some(metrics) = payload
        .get("modelMetrics")
        .or_else(|| event.get("modelMetrics"))
        .and_then(Value::as_object)
    else {
        return;
    };

    let current_model = payload
        .get("currentModel")
        .or_else(|| event.get("currentModel"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|model| !model.is_empty() && *model != "auto")
        .map(str::to_string);

    for (model, entry) in metrics {
        let Some(usage) = entry.get("usage") else {
            continue;
        };
        let read = |key: &str| usage.get(key).and_then(Value::as_i64).unwrap_or(0).max(0);
        let tracker_model = model.trim().to_string();
        let attributed_model = match tracker_model.as_str() {
            "" | "auto" => current_model.clone(),
            _ => Some(tracker_model.clone()),
        };
        let shutdown = ShutdownUsage {
            event_id: event_id.clone(),
            timestamp_ms,
            model: tracker_model,
            attributed_model,
            input: read("inputTokens"),
            output: read("outputTokens"),
            cache_read: read("cacheReadTokens"),
            cache_write: read("cacheWriteTokens"),
            reasoning: read("reasoningTokens"),
        };
        if shutdown.input == 0
            && shutdown.output == 0
            && shutdown.cache_read == 0
            && shutdown.cache_write == 0
            && shutdown.reasoning == 0
        {
            continue;
        }
        out.push(shutdown);
    }
}

fn parse_iso8601_timestamp_ms(value: &str) -> Option<i64> {
    DateTime::parse_from_rfc3339(value)
        .map(|timestamp| timestamp.timestamp_millis())
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f")
                .ok()
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
        .or_else(|| {
            // SQLite's default datetime() text form is space-separated and may
            // carry fractional seconds ("2026-07-01 12:34:56.789"); without this
            // branch it fails every parse above and the session lands in 1970.
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|timestamp| timestamp.and_utc().timestamp_millis())
        })
        .or_else(|| {
            let numeric = value.parse::<i64>().ok()?;
            // Distinguish seconds vs milliseconds: values < 10 billion are
            // assumed to be Unix seconds (common in SQLite), otherwise millis.
            if numeric > 10_000_000_000 {
                Some(numeric)
            } else {
                Some(numeric.saturating_mul(1000))
            }
        })
}

#[cfg(test)]
mod tests;
