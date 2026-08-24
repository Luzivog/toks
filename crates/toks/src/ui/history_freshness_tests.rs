use chrono::{TimeZone, Utc};

use super::history_freshness::history_freshness_text;
use crate::history_refresh::HistoryRefreshState;

fn now() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0)
        .single()
        .expect("valid time")
}

#[test]
fn catch_up_distinguishes_backfill_from_recent_updates() {
    let mut state = HistoryRefreshState::ready();
    state.catching_up(2, None, toks_core::history::CatchUpRetry::Immediate);
    assert_eq!(
        history_freshness_text(&state, now()).as_deref(),
        Some("Indexing past usage · 2 sources left")
    );

    state.updating_recent();
    assert_eq!(
        history_freshness_text(&state, now()).as_deref(),
        Some("Updating recent usage")
    );
}

#[test]
fn last_good_is_explicit_and_keeps_capture_context() {
    let mut state = HistoryRefreshState::ready();
    state.complete(Some(1_777_000_000_000));
    state.busy_using_last_good(None);
    let text = history_freshness_text(&state, now()).expect("freshness text");
    assert!(text.starts_with("Using saved usage · Updated "));
}

#[test]
fn a_fresh_but_unsaved_snapshot_is_not_called_stale() {
    let mut state = HistoryRefreshState::ready();
    state.fresh_save_delayed(None);
    assert_eq!(
        history_freshness_text(&state, now()).as_deref(),
        Some("Fresh usage · local save delayed")
    );
}
