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
