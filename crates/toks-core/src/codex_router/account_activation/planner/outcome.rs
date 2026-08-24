use crate::accounts::AccountId;

use super::super::job;
use super::super::model::{Document, FailureReason, JobPhase};

pub(in crate::codex_router::account_activation) fn finish(
    document: &mut Document,
    id: &str,
    success: bool,
    reason: FailureReason,
    now_ms: i64,
) -> bool {
    for state in document.accounts.values_mut() {
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
            job::finish(&mut state.active_until_ms, job, success, reason, now_ms);
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
