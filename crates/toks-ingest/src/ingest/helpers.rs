use crate::{
    apply_pricing_if_available, bucket_tz, pricing, scanner, sessions, should_keep_deduped_message,
    UnifiedMessage,
};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Re-key every message onto the device's pinned bucketing timezone.
///
/// The parsers derive `date` from `chrono::Local`, read afresh on every scan,
/// so which day a message lands in changes when the machine's zone does. This
/// is the one pass that knows the user's settings and sees every message, so it
/// is where the day key gets fixed to something a rescan cannot move.
///
/// Runs after the source cache is written on purpose: the cache stores raw
/// parser output and `refresh_derived_fields` re-derives `date` on every load,
/// so cached entries never carry a stale day key past this point and changing
/// the pinned zone needs no cache invalidation.
///
/// **No-op when nothing is pinned.** An unpinned device must report exactly
/// what it reported before, so the pass is skipped rather than re-derived
/// through `Local`.
pub(crate) fn rebucket_days(
    messages: &mut [UnifiedMessage],
    scanner_settings: &scanner::ScannerSettings,
) {
    let timezone = bucket_tz::BucketTimezone::from_scanner_settings(scanner_settings);
    if !timezone.is_pinned() {
        return;
    }

    for message in messages.iter_mut() {
        message.rebucket_date(&timezone);
    }
}

pub(crate) fn dedupe_latest_trae_messages(
    mut messages: Vec<UnifiedMessage>,
) -> Vec<UnifiedMessage> {
    let mut latest_by_session: HashMap<String, UnifiedMessage> = HashMap::new();

    for message in messages.drain(..) {
        let session_id = message.session_id.clone();
        match latest_by_session.get_mut(&session_id) {
            Some(existing) => {
                let should_replace = message.timestamp > existing.timestamp
                    || (message.timestamp == existing.timestamp
                        && message.dedup_key.as_ref().is_some_and(|key| {
                            existing
                                .dedup_key
                                .as_ref()
                                .is_none_or(|existing_key| key > existing_key)
                        }));
                if should_replace {
                    *existing = message;
                }
            }
            None => {
                let _ = latest_by_session.insert(session_id, message);
            }
        }
    }

    let mut deduped: Vec<UnifiedMessage> = latest_by_session.into_values().collect();
    deduped.sort_unstable_by(|a, b| {
        a.session_id
            .cmp(&b.session_id)
            .then_with(|| a.timestamp.cmp(&b.timestamp))
    });
    deduped
}

pub(crate) fn partition_workbuddy_paths(paths: &[PathBuf]) -> (Vec<&PathBuf>, Vec<&PathBuf>) {
    paths
        .iter()
        .partition(|path| sessions::workbuddy::is_detailed_workbuddy_source(path))
}

pub(crate) fn merge_workbuddy_messages(
    detailed_messages: Vec<UnifiedMessage>,
    fallback_messages: Vec<UnifiedMessage>,
) -> Vec<UnifiedMessage> {
    // The SQLite fallback carries ONE cumulative row per session (dated solely by
    // `updated_at`), while the detailed JSONL carries accurate per-message rows.
    // A fallback row is redundant exactly when its session already has detailed
    // coverage — independent of which calendar day `updated_at` lands on. Keying
    // this on the session (not the date) fixes two failures of the old
    // date-overlap check: it no longer double-counts a session whose aggregate
    // lands on a day with no detailed rows, and no longer drops a fallback-only
    // session that merely shares a day with unrelated detailed activity. Both
    // parsers derive `session_id` from the same WorkBuddy session identifier, so
    // the keys are directly comparable.
    let detailed_sessions: HashSet<String> = detailed_messages
        .iter()
        .filter(|message| !message.session_id.is_empty())
        .map(|message| message.session_id.clone())
        .collect();
    let mut seen: HashSet<String> = HashSet::new();
    let mut merged: Vec<UnifiedMessage> = detailed_messages
        .into_iter()
        .filter(|message| should_keep_deduped_message(&mut seen, message))
        .collect();

    merged.extend(fallback_messages.into_iter().filter(|message| {
        !detailed_sessions.contains(&message.session_id)
            && should_keep_deduped_message(&mut seen, message)
    }));
    merged
}

pub(crate) fn is_headless_path(path: &Path, headless_roots: &[PathBuf]) -> bool {
    headless_roots.iter().any(|root| path.starts_with(root))
}

pub(crate) fn apply_headless_agent(message: &mut UnifiedMessage, is_headless: bool) {
    if is_headless && message.agent.is_none() {
        message.agent = Some("headless".to_string());
    }
}

pub(crate) fn parse_hermes_sqlite_with_pricing(
    db_path: &Path,
    pricing: Option<&pricing::PricingService>,
) -> Vec<UnifiedMessage> {
    sessions::hermes::parse_hermes_sqlite(db_path)
        .into_iter()
        .map(|mut msg| {
            if msg.cost <= 0.0 {
                apply_pricing_if_available(&mut msg, pricing);
            }
            msg
        })
        .collect()
}
