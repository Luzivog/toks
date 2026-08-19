use std::time::Duration as StdDuration;

use tokscope_core::history::CatchUpRetry;

const HISTORY_REFRESH: StdDuration = StdDuration::from_secs(60);
const ACTIVE_SOURCE_RETRY: StdDuration = StdDuration::from_secs(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HistoryActivity {
    Ready,
    UpdatingRecent,
    IndexingPast { pending_sources: usize },
    FreshSaveDelayed,
    BusyUsingLastGood,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HistoryRefreshState {
    activity: HistoryActivity,
    captured_through_ms: Option<i64>,
    catch_up_retry: CatchUpRetry,
}

impl HistoryRefreshState {
    pub(crate) fn ready() -> Self {
        Self {
            activity: HistoryActivity::Ready,
            captured_through_ms: None,
            catch_up_retry: CatchUpRetry::ShortBackoff,
        }
    }

    pub(crate) fn updating_recent(&mut self) {
        self.activity = HistoryActivity::UpdatingRecent;
    }

    pub(crate) fn begin_cycle(&mut self) {
        if !self.is_catching_up() {
            self.updating_recent();
        }
    }

    pub(crate) fn complete(&mut self, captured_through_ms: Option<i64>) {
        self.activity = HistoryActivity::Ready;
        self.merge_capture(captured_through_ms);
    }

    pub(crate) fn catching_up(
        &mut self,
        pending_sources: usize,
        captured_through_ms: Option<i64>,
        retry: CatchUpRetry,
    ) {
        self.activity = HistoryActivity::IndexingPast { pending_sources };
        self.catch_up_retry = retry;
        self.merge_capture(captured_through_ms);
    }

    pub(crate) fn fresh_save_delayed(&mut self, captured_through_ms: Option<i64>) {
        self.activity = HistoryActivity::FreshSaveDelayed;
        self.merge_capture(captured_through_ms);
    }

    pub(crate) fn busy_using_last_good(&mut self, captured_through_ms: Option<i64>) {
        self.activity = HistoryActivity::BusyUsingLastGood;
        self.merge_capture(captured_through_ms);
    }

    pub(crate) fn activity(&self) -> HistoryActivity {
        self.activity
    }

    pub(crate) fn captured_through_ms(&self) -> Option<i64> {
        self.captured_through_ms
    }

    pub(crate) fn is_catching_up(&self) -> bool {
        matches!(self.activity, HistoryActivity::IndexingPast { .. })
    }

    pub(crate) fn next_delay(&self, cycle_elapsed: StdDuration) -> StdDuration {
        match (&self.activity, self.catch_up_retry) {
            (HistoryActivity::IndexingPast { .. }, CatchUpRetry::Immediate) => StdDuration::ZERO,
            (HistoryActivity::IndexingPast { .. }, CatchUpRetry::ShortBackoff) => {
                ACTIVE_SOURCE_RETRY
            }
            _ => HISTORY_REFRESH.saturating_sub(cycle_elapsed),
        }
    }

    fn merge_capture(&mut self, captured_through_ms: Option<i64>) {
        self.captured_through_ms = match (self.captured_through_ms, captured_through_ms) {
            (Some(current), Some(next)) => Some(current.max(next)),
            (current, next) => current.or(next),
        };
    }
}

#[cfg(test)]
mod tests;
