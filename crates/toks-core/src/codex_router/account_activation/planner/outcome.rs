use crate::accounts::AccountId;

use crate::codex_router::account_activation::job;
use crate::codex_router::account_activation::model::{
    Document, FailureReason, JobPhase, ManualRoute,
};
use crate::codex_router::account_activation::status::{ManualTestOutcome, ManualTestReceipt};

pub(in crate::codex_router::account_activation) fn finish(
    document: &mut Document,
    id: &str,
    success: bool,
    reason: FailureReason,
    now_ms: i64,
) -> bool {
    for (account, state) in &mut document.accounts {
        if let Some(job) = state
            .automatic
            .as_mut()
            .filter(|job| job.id == id && job::running(job))
        {
            job::finish(&mut state.active_until_ms, job, success, reason, now_ms);
            return true;
        }
        if let Some(job) = state
            .manual
            .as_mut()
            .filter(|job| job.id == id && job::running(job))
        {
            let started_at_ms = match job.phase {
                JobPhase::Running { started_at_ms, .. } => started_at_ms,
                _ => unreachable!("manual job was filtered to running"),
            };
            let route = job.manual_route.clone();
            let verified = matches!(
                &route,
                Some(ManualRoute::Routed {
                    observed_account,
                    ..
                }) if observed_account == account
            );
            let effective_success = success && verified;
            let effective_reason = if success && !verified {
                FailureReason::RouteUnverified
            } else {
                reason
            };
            state.manual_receipt = Some(ManualTestReceipt {
                requested_account: account.clone(),
                observed_account: match &route {
                    Some(ManualRoute::Routed {
                        observed_account, ..
                    }) => Some(observed_account.clone()),
                    _ => None,
                },
                thread_id: route.as_ref().map(|route| route.thread_id().clone()),
                started_at_ms,
                routed_at_ms: match &route {
                    Some(ManualRoute::Routed { routed_at_ms, .. }) => Some(*routed_at_ms),
                    _ => None,
                },
                completed_at_ms: now_ms,
                outcome: if effective_success {
                    ManualTestOutcome::Succeeded
                } else {
                    ManualTestOutcome::Failed
                },
            });
            job::finish(
                &mut state.active_until_ms,
                job,
                effective_success,
                effective_reason,
                now_ms,
            );
            return true;
        }
    }
    false
}

pub(in crate::codex_router::account_activation) fn fail_pending_manual(
    document: &mut Document,
    account: &AccountId,
    reason: FailureReason,
    now_ms: i64,
) -> bool {
    let Some(job) = document
        .accounts
        .get_mut(account)
        .and_then(|state| state.manual.as_mut())
    else {
        return false;
    };
    if !matches!(job.phase, JobPhase::Pending { .. }) {
        return false;
    }
    job.owner = None;
    job.phase = JobPhase::NeedsAttention {
        failed_at_ms: now_ms,
        reason,
    };
    true
}
