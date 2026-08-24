use std::collections::BTreeSet;

use crate::rotation::{RotationSettings, UnixMillis, WaitingThread};

use super::state::ResumeState;

pub(super) fn waiting_candidates(
    settings: &RotationSettings,
    waiting: &[WaitingThread],
    state: &ResumeState,
    now: UnixMillis,
) -> Vec<WaitingThread> {
    let live = waiting
        .iter()
        .map(|waiting| waiting.thread_id.clone())
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    settings
        .waiting_priority()
        .iter()
        .chain(waiting.iter().map(|waiting| &waiting.thread_id))
        .filter(|thread| {
            seen.insert((*thread).clone())
                && live.contains(*thread)
                && !settings.cancelled_threads().contains(*thread)
                && !state.attempts.contains_key(*thread)
                && waiting_for(waiting, thread).is_some_and(|waiting| {
                    state
                        .retry_after
                        .get(&waiting.waiting_id)
                        .is_none_or(|after| *after <= now)
                })
        })
        .filter_map(|thread| waiting_for(waiting, thread).cloned())
        .collect()
}

fn waiting_for<'a>(
    waiting: &'a [WaitingThread],
    thread: &crate::rotation::ThreadId,
) -> Option<&'a WaitingThread> {
    waiting.iter().find(|waiting| &waiting.thread_id == thread)
}
