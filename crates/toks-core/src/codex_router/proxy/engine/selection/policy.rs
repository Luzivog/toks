use std::collections::BTreeSet;

use crate::accounts::AccountId;
use crate::rotation::{
    ResumeRoute, RotationRuntime, RotationSettings, ThreadId, ThreadOwnership, UnixMillis,
};

use super::RouteSelection;
use crate::codex_router::proxy::headers::ResumeMarker;

pub(super) fn selected_account(
    settings: &RotationSettings,
    runtime: &RotationRuntime,
    discovered: &[AccountId],
    thread: Option<&ThreadId>,
    marker: ResumeMarker<'_>,
    skipped: &BTreeSet<AccountId>,
    at: UnixMillis,
) -> RouteSelection<AccountId> {
    if !settings.enabled() {
        return if marker.is_present() {
            RouteSelection::ResumeDenied
        } else {
            RouteSelection::Unavailable
        };
    }
    if marker == ResumeMarker::Invalid {
        return RouteSelection::ResumeDenied;
    }
    if thread.is_none() {
        if let Some(attempt) = marker.attempt() {
            return runtime
                .resume_attempt_binding(attempt)
                .filter(|(account, thread)| {
                    discovered.contains(account)
                        && !settings.excluded().contains(account)
                        && !skipped.contains(account)
                        && (runtime.is_available(account, at)
                            || runtime.can_drain(account, thread, at))
                })
                .map_or(RouteSelection::ResumeDenied, |(account, _)| {
                    RouteSelection::Selected(account)
                });
        }
        if marker.is_present() {
            return RouteSelection::ResumeDenied;
        }
    }
    if let Some(thread) = thread {
        match runtime.resume_route(thread, marker.attempt()) {
            ResumeRoute::Authorized(account) => {
                if discovered.contains(&account)
                    && !settings.excluded().contains(&account)
                    && !skipped.contains(&account)
                    && (runtime.is_available(&account, at)
                        || runtime.can_drain(&account, thread, at))
                {
                    return RouteSelection::Selected(account);
                }
                return RouteSelection::ResumeDenied;
            }
            ResumeRoute::Denied => return RouteSelection::ResumeDenied,
            ResumeRoute::Unclaimed => {}
        }
        match runtime.thread_ownership(thread) {
            ThreadOwnership::Owned(account) => {
                if discovered.contains(&account)
                    && !settings.excluded().contains(&account)
                    && !skipped.contains(&account)
                    && (runtime.is_available(&account, at)
                        || runtime.can_drain(&account, thread, at))
                {
                    return RouteSelection::Selected(account);
                }
                return RouteSelection::Unavailable;
            }
            ThreadOwnership::Conflicting => return RouteSelection::Unavailable,
            ThreadOwnership::Unowned => {}
        }
    }
    thread
        .and_then(|thread| runtime.draining_account(thread, at))
        .filter(|account| {
            discovered.contains(account)
                && !settings.excluded().contains(account)
                && !skipped.contains(account)
        })
        .or_else(|| {
            settings
                .priority()
                .iter()
                .find(|account| {
                    discovered.contains(account)
                        && !settings.excluded().contains(account)
                        && !skipped.contains(account)
                        && runtime.is_available(account, at)
                })
                .cloned()
        })
        .map_or(RouteSelection::Unavailable, RouteSelection::Selected)
}
