use anyhow::Result;

use crate::accounts::AccountId;
use crate::rotation::ThreadId;
use crate::storage::StoreUpdate;

use super::model::{JobPhase, ManualRoute, TASK_TIMEOUT_MS};
use super::store::Store;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::codex_router) enum RouteClaim {
    Selected(AccountId),
    Denied,
}

impl Store {
    pub(in crate::codex_router) fn claim_route(
        &self,
        attempt: &str,
        thread: &ThreadId,
        now_ms: i64,
    ) -> Result<RouteClaim> {
        if !canonical_attempt(attempt) {
            return Ok(RouteClaim::Denied);
        }
        self.update(|document| {
            let Some((account, job)) = document.accounts.iter_mut().find_map(|(account, state)| {
                state
                    .manual
                    .as_mut()
                    .filter(|job| job.id == attempt)
                    .map(|job| (account, job))
            }) else {
                return StoreUpdate::Unchanged(RouteClaim::Denied);
            };
            let JobPhase::Running { started_at_ms, .. } = job.phase else {
                return StoreUpdate::Unchanged(RouteClaim::Denied);
            };
            if now_ms.saturating_sub(started_at_ms) >= TASK_TIMEOUT_MS
                || job.owner.is_none_or(|owner| !owner.is_alive())
            {
                return StoreUpdate::Unchanged(RouteClaim::Denied);
            }
            match &job.manual_route {
                Some(route) if route.thread_id() == thread => {
                    StoreUpdate::Unchanged(RouteClaim::Selected(account.clone()))
                }
                Some(_) => StoreUpdate::Unchanged(RouteClaim::Denied),
                None => {
                    job.manual_route = Some(ManualRoute::Bound {
                        thread_id: thread.clone(),
                        bound_at_ms: now_ms,
                    });
                    StoreUpdate::Changed(RouteClaim::Selected(account.clone()))
                }
            }
        })
    }

    pub(in crate::codex_router) fn observe_route(
        &self,
        attempt: &str,
        thread: &ThreadId,
        observed_account: &AccountId,
        now_ms: i64,
    ) -> Result<()> {
        self.update(|document| {
            let Some((requested, job)) =
                document.accounts.iter_mut().find_map(|(account, state)| {
                    state
                        .manual
                        .as_mut()
                        .filter(|job| job.id == attempt)
                        .map(|job| (account, job))
                })
            else {
                return StoreUpdate::Unchanged(Err(anyhow::anyhow!(
                    "activation authorization no longer exists"
                )));
            };
            if requested != observed_account || !matches!(job.phase, JobPhase::Running { .. }) {
                return StoreUpdate::Unchanged(Err(anyhow::anyhow!(
                    "activation route does not match its requested account"
                )));
            }
            match &job.manual_route {
                Some(ManualRoute::Bound {
                    thread_id,
                    bound_at_ms,
                }) if thread_id == thread => {
                    job.manual_route = Some(ManualRoute::Routed {
                        thread_id: thread.clone(),
                        bound_at_ms: *bound_at_ms,
                        routed_at_ms: now_ms,
                        observed_account: observed_account.clone(),
                    });
                    StoreUpdate::Changed(Ok(()))
                }
                Some(ManualRoute::Routed {
                    thread_id,
                    observed_account: current,
                    ..
                }) if thread_id == thread && current == observed_account => {
                    StoreUpdate::Unchanged(Ok(()))
                }
                _ => StoreUpdate::Unchanged(Err(anyhow::anyhow!(
                    "activation authorization was already consumed"
                ))),
            }
        })?
    }
}

fn canonical_attempt(attempt: &str) -> bool {
    uuid::Uuid::parse_str(attempt).is_ok_and(|parsed| parsed.to_string() == attempt)
}

#[cfg(test)]
impl Store {
    pub(in crate::codex_router) fn seed_running_manual_for_test(
        &self,
        account: AccountId,
        attempt: &str,
        started_at_ms: i64,
    ) -> Result<()> {
        use crate::accounts::CredentialProfileId;

        use super::model::{Job, JobKind};
        use super::owner::ProcessOwner;

        self.update(|document| {
            document.accounts.entry(account).or_default().manual = Some(Job {
                id: attempt.to_owned(),
                kind: JobKind::Manual,
                launches: 1,
                owner: ProcessOwner::current(),
                phase: JobPhase::Running {
                    started_at_ms,
                    profile_id: CredentialProfileId::new("test-profile"),
                },
                manual_route: None,
            });
            StoreUpdate::Changed(())
        })
    }

    pub(super) fn finish_for_test(
        &self,
        attempt: &str,
        result: std::result::Result<(), super::model::FailureReason>,
        now_ms: i64,
    ) -> Result<()> {
        self.update(|document| {
            let (success, reason) = match result {
                Ok(()) => (true, super::model::FailureReason::Unsuccessful),
                Err(reason) => (false, reason),
            };
            let changed = super::planner::finish(document, attempt, success, reason, now_ms);
            StoreUpdate::from_changed((), changed)
        })
    }

    pub(in crate::codex_router) fn finish_success_for_test(
        &self,
        attempt: &str,
        now_ms: i64,
    ) -> Result<()> {
        self.finish_for_test(attempt, Ok(()), now_ms)
    }

    pub(in crate::codex_router) fn status_for_test(
        &self,
        account: &AccountId,
        now_ms: i64,
    ) -> Result<super::status::AccountActivationStatus> {
        self.update(|document| {
            let before = document.clone();
            super::requests::reconcile_account(document, account, now_ms);
            let status = super::requests::status(document, account);
            StoreUpdate::from_changed(status, *document != before)
        })
    }
}
