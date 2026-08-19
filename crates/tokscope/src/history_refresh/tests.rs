use std::time::Duration;

use super::{HistoryActivity, HistoryRefreshState};
use tokscope_core::history::CatchUpRetry;

#[test]
fn catch_up_runs_again_immediately_and_keeps_latest_capture() {
    let mut state = HistoryRefreshState::ready();
    state.updating_recent();
    state.catching_up(12, Some(1_777_000_000_000), CatchUpRetry::Immediate);

    assert_eq!(
        state.activity(),
        HistoryActivity::IndexingPast {
            pending_sources: 12
        }
    );
    assert_eq!(state.captured_through_ms(), Some(1_777_000_000_000));
    assert_eq!(state.next_delay(Duration::from_secs(2)), Duration::ZERO);
    state.begin_cycle();
    assert_eq!(
        state.activity(),
        HistoryActivity::IndexingPast {
            pending_sources: 12
        }
    );
}

#[test]
fn incomplete_active_source_uses_a_short_backoff() {
    let mut state = HistoryRefreshState::ready();
    state.catching_up(1, Some(42), CatchUpRetry::ShortBackoff);

    assert_eq!(state.next_delay(Duration::ZERO), Duration::from_secs(1));
    assert_eq!(
        state.next_delay(Duration::from_secs(20)),
        Duration::from_secs(1)
    );
}

#[test]
fn ready_cadence_is_measured_from_cycle_start() {
    let mut state = HistoryRefreshState::ready();
    state.complete(Some(42));

    assert_eq!(
        state.next_delay(Duration::from_secs(17)),
        Duration::from_secs(43)
    );
    assert_eq!(state.next_delay(Duration::from_secs(90)), Duration::ZERO);
}

#[test]
fn busy_state_retains_last_good_capture() {
    let mut state = HistoryRefreshState::ready();
    state.complete(Some(42));
    state.updating_recent();
    state.busy_using_last_good(None);

    assert_eq!(state.activity(), HistoryActivity::BusyUsingLastGood);
    assert_eq!(state.captured_through_ms(), Some(42));
}

#[test]
fn an_older_fallback_never_moves_freshness_backwards() {
    let mut state = HistoryRefreshState::ready();
    state.complete(Some(100));
    state.busy_using_last_good(Some(90));

    assert_eq!(state.captured_through_ms(), Some(100));
}
