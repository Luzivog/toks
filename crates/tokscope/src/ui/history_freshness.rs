use chrono::{DateTime, Datelike, Local, TimeZone, Utc};

use crate::history_refresh::{HistoryActivity, HistoryRefreshState};

pub(super) fn history_freshness_text(
    state: &HistoryRefreshState,
    now: DateTime<Utc>,
) -> Option<String> {
    let captured = state
        .captured_through_ms()
        .and_then(|millis| Local.timestamp_millis_opt(millis).single())
        .map(|captured| captured_label(captured, now.with_timezone(&Local)));

    let activity = match state.activity() {
        HistoryActivity::Ready => None,
        HistoryActivity::UpdatingRecent => Some("Updating recent usage".to_string()),
        HistoryActivity::IndexingPast { pending_sources } => Some(format!(
            "Indexing past usage · {pending_sources} {} left",
            if pending_sources == 1 {
                "source"
            } else {
                "sources"
            }
        )),
        HistoryActivity::FreshSaveDelayed => Some("Fresh usage · local save delayed".to_string()),
        HistoryActivity::BusyUsingLastGood => Some("Using saved usage".to_string()),
    };

    match (activity, captured) {
        (Some(activity), Some(captured)) => Some(format!("{activity} · {captured}")),
        (Some(activity), None) => Some(activity),
        (None, captured) => captured,
    }
}

fn captured_label(captured: DateTime<Local>, now: DateTime<Local>) -> String {
    if captured.year() == now.year() && captured.ordinal() == now.ordinal() {
        format!("Updated {}", captured.format("%H:%M"))
    } else {
        format!("Updated {}", captured.format("%b %-d, %H:%M"))
    }
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::history_freshness_text;
    use crate::history_refresh::HistoryRefreshState;

    fn now() -> chrono::DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 19, 12, 0, 0)
            .single()
            .expect("valid time")
    }

    #[test]
    fn catch_up_distinguishes_backfill_from_recent_updates() {
        let mut state = HistoryRefreshState::ready();
        state.catching_up(2, None, tokscope_core::history::CatchUpRetry::Immediate);
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
}
