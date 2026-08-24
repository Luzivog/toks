use super::model::{
    FailureReason, Job, JobKind, JobPhase, PROVISIONAL_WEEK_MS, RETRY_DELAYS_MS, TASK_TIMEOUT_MS,
};
use super::{AutomaticTestStatus, ManualTestStatus};

pub(super) fn active(job: &Job) -> bool {
    matches!(
        job.phase,
        JobPhase::Pending { .. } | JobPhase::Running { .. } | JobPhase::Checking { .. }
    )
}

pub(super) fn running(job: &Job) -> bool {
    matches!(job.phase, JobPhase::Running { .. })
}

pub(super) fn reconcile_timeout(job: &mut Job, now_ms: i64) {
    let JobPhase::Running { started_at_ms, .. } = job.phase else {
        return;
    };
    if now_ms.saturating_sub(started_at_ms) >= TASK_TIMEOUT_MS {
        needs_attention(job, FailureReason::TimedOut, now_ms);
    }
}

pub(super) fn reconcile_owner(job: &mut Job, now_ms: i64) {
    let requires_owner = matches!(job.phase, JobPhase::Running { .. })
        || matches!(job.kind, JobKind::Manual) && matches!(job.phase, JobPhase::Pending { .. });
    if requires_owner && job.owner.is_none_or(|owner| !owner.is_alive()) {
        needs_attention(job, FailureReason::Interrupted, now_ms);
    }
}

pub(super) fn finish(
    active_until_ms: &mut Option<i64>,
    job: &mut Job,
    success: bool,
    reason: FailureReason,
    now_ms: i64,
) {
    if success {
        if active_until_ms.is_none_or(|until| until <= now_ms) {
            *active_until_ms = Some(now_ms.saturating_add(PROVISIONAL_WEEK_MS));
        }
        job.phase = JobPhase::Succeeded {
            completed_at_ms: now_ms,
        };
        job.owner = None;
    } else {
        fail(job, reason, now_ms);
    }
}

fn fail(job: &mut Job, reason: FailureReason, now_ms: i64) {
    job.owner = None;
    if safe_to_retry(reason)
        && matches!(job.kind, JobKind::Automatic { .. })
        && usize::from(job.launches) <= RETRY_DELAYS_MS.len()
    {
        let delay = RETRY_DELAYS_MS[usize::from(job.launches.saturating_sub(1))];
        job.phase = JobPhase::Checking {
            failed_at_ms: now_ms,
            not_before_ms: now_ms.saturating_add(delay),
        };
    } else {
        needs_attention(job, reason, now_ms);
    }
}

fn safe_to_retry(reason: FailureReason) -> bool {
    matches!(
        reason,
        FailureReason::ModelUnavailable
            | FailureReason::ProfileUnavailable
            | FailureReason::SpawnFailed
    )
}

fn needs_attention(job: &mut Job, reason: FailureReason, now_ms: i64) {
    job.owner = None;
    job.phase = JobPhase::NeedsAttention {
        failed_at_ms: now_ms,
        reason,
    };
}

pub(super) fn automatic_status(job: &Job) -> AutomaticTestStatus {
    match job.phase {
        JobPhase::Pending { .. } | JobPhase::Checking { .. } => AutomaticTestStatus::Pending,
        JobPhase::Running { .. } => AutomaticTestStatus::Running,
        JobPhase::Succeeded { .. } => AutomaticTestStatus::Succeeded,
        JobPhase::NeedsAttention { .. } => AutomaticTestStatus::NeedsAttention,
    }
}

pub(super) fn manual_status(job: &Job) -> ManualTestStatus {
    match job.phase {
        JobPhase::Pending { .. } => ManualTestStatus::Pending,
        JobPhase::Running { .. } => ManualTestStatus::Running,
        JobPhase::Checking { .. } | JobPhase::NeedsAttention { .. } => ManualTestStatus::Failed,
        JobPhase::Succeeded { .. } => ManualTestStatus::Succeeded,
    }
}
