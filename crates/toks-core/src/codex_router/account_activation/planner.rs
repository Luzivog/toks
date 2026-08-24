use std::collections::BTreeMap;

use crate::accounts::AccountId;

use super::authority::Authority;
use super::job;
use super::model::{AccountState, Document, FailureReason, Job, JobKind, JobPhase, Launch};

mod outcome;
pub(super) use outcome::{fail_pending_manual, finish};

pub(super) fn observe(
    document: &mut Document,
    authorities: &[Authority],
    now_ms: i64,
) -> Vec<Launch> {
    let authorities = authorities
        .iter()
        .map(|authority| (authority.account.clone(), authority))
        .collect::<BTreeMap<_, _>>();
    let mut launches = Vec::new();
    for (account, authority) in &authorities {
        let disabled = document.disabled.contains(account);
        let state = document.accounts.entry(account.clone()).or_default();
        adopt_fixed_reset(state, authority, now_ms);
        reconcile_active(state, now_ms);
        if !disabled && !state.manual.as_ref().is_some_and(job::active) {
            ensure_automatic(state, authority, now_ms);
        }
        claim(account, state, authority, now_ms, !disabled, &mut launches);
    }
    launches
}

fn adopt_fixed_reset(state: &mut AccountState, authority: &Authority, now_ms: i64) {
    let Some(weekly) = authority.weekly.filter(|weekly| weekly.percent_used > 0.0) else {
        return;
    };
    state.active_until_ms = Some(weekly.resets_at_ms);
    if let Some(job) = state.automatic.as_mut() {
        if matches!(
            job.phase,
            JobPhase::Pending { .. } | JobPhase::Checking { .. } | JobPhase::NeedsAttention { .. }
        ) {
            job.phase = JobPhase::Succeeded {
                completed_at_ms: now_ms,
            };
        }
    }
}

fn reconcile_active(state: &mut AccountState, now_ms: i64) {
    for job in [&mut state.automatic, &mut state.manual]
        .into_iter()
        .flatten()
    {
        job::reconcile_owner(job, now_ms);
        job::reconcile_timeout(job, now_ms);
    }
}

fn ensure_automatic(state: &mut AccountState, authority: &Authority, now_ms: i64) {
    let Some(weekly) = authority.weekly else {
        return;
    };
    if weekly.percent_used != 0.0 || state.active_until_ms.is_some_and(|until| until > now_ms) {
        return;
    }
    let predecessor = state.active_until_ms;
    let current = state.automatic.as_ref().is_some_and(|job| {
        matches!(job.kind, JobKind::Automatic { predecessor_active_until_ms } if predecessor_active_until_ms == predecessor)
    });
    if !current {
        state.automatic = Some(Job {
            id: uuid::Uuid::new_v4().to_string(),
            kind: JobKind::Automatic {
                predecessor_active_until_ms: predecessor,
            },
            launches: 0,
            owner: None,
            phase: JobPhase::Pending {
                not_before_ms: now_ms,
            },
        });
    }
}

fn claim(
    account: &AccountId,
    state: &mut AccountState,
    authority: &Authority,
    now_ms: i64,
    automatic_enabled: bool,
    launches: &mut Vec<Launch>,
) {
    if state.automatic.as_ref().is_some_and(job::running)
        || state.manual.as_ref().is_some_and(job::running)
    {
        return;
    }
    if let Some(job) = state
        .manual
        .as_mut()
        .filter(|job| due(job, authority, now_ms))
    {
        launch(account, job, authority, now_ms, launches);
        return;
    }
    if automatic_enabled {
        if let Some(job) = state
            .automatic
            .as_mut()
            .filter(|job| due(job, authority, now_ms))
        {
            launch(account, job, authority, now_ms, launches);
        }
    }
}

fn launch(
    account: &AccountId,
    job: &mut Job,
    authority: &Authority,
    now_ms: i64,
    launches: &mut Vec<Launch>,
) {
    let Some(owner) = super::owner::ProcessOwner::current() else {
        job.phase = JobPhase::NeedsAttention {
            failed_at_ms: now_ms,
            reason: FailureReason::SpawnFailed,
        };
        return;
    };
    job.launches = job.launches.saturating_add(1);
    job.owner = Some(owner);
    job.phase = JobPhase::Running {
        started_at_ms: now_ms,
        profile_id: authority.profile_id.clone(),
    };
    launches.push(Launch {
        id: job.id.clone(),
        account: account.clone(),
        profile_id: authority.profile_id.clone(),
    });
}

fn due(job: &Job, authority: &Authority, now_ms: i64) -> bool {
    match job.phase {
        JobPhase::Pending { not_before_ms } => now_ms >= not_before_ms,
        JobPhase::Checking {
            failed_at_ms,
            not_before_ms,
        } => now_ms >= not_before_ms && authority.fetched_at_ms > failed_at_ms,
        _ => false,
    }
}
