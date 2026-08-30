use crate::accounts::AccountId;

use super::job;
use super::model::{Document, Job, JobKind, JobPhase};
use super::owner::ProcessOwner;
use super::{AccountActivationStatus, AutomaticTestStatus, ManualRequest, ManualTestStatus};

pub(super) fn manual(
    document: &mut Document,
    account: &AccountId,
    owner: ProcessOwner,
    now_ms: i64,
) -> ManualRequest {
    reconcile_account(document, account, now_ms);
    let state = document.accounts.entry(account.clone()).or_default();
    if state.manual.as_ref().is_some_and(job::active)
        || state.automatic.as_ref().is_some_and(job::active)
    {
        return ManualRequest::AlreadyRunning;
    }
    state.manual = Some(Job {
        id: uuid::Uuid::new_v4().to_string(),
        kind: JobKind::Manual,
        launches: 0,
        owner: Some(owner),
        phase: JobPhase::Pending {
            not_before_ms: now_ms,
        },
        manual_route: None,
    });
    ManualRequest::Queued
}

pub(super) fn reconcile_account(document: &mut Document, account: &AccountId, now_ms: i64) {
    let Some(state) = document.accounts.get_mut(account) else {
        return;
    };
    if let Some(job) = state.automatic.as_mut() {
        job::reconcile_owner(job, now_ms);
        job::reconcile_timeout(job, now_ms);
    }
    job::reconcile_manual(account, state, now_ms);
}

pub(super) fn set_automatic(document: &mut Document, account: &AccountId, enabled: bool) {
    if enabled {
        document.disabled.remove(account);
    } else {
        document.disabled.insert(account.clone());
        let state = document.accounts.entry(account.clone()).or_default();
        if state
            .automatic
            .as_ref()
            .is_some_and(|job| !job::running(job))
        {
            state.automatic = None;
        }
    }
}

pub(super) fn status(document: &Document, account: &AccountId) -> AccountActivationStatus {
    let state = document.accounts.get(account);
    AccountActivationStatus {
        automatic_enabled: !document.disabled.contains(account),
        active_until_ms: state.and_then(|state| state.active_until_ms),
        automatic: state
            .and_then(|state| state.automatic.as_ref())
            .map_or(AutomaticTestStatus::Ready, job::automatic_status),
        manual: state
            .and_then(|state| state.manual.as_ref())
            .map_or(ManualTestStatus::Ready, job::manual_status),
        manual_receipt: state.and_then(|state| state.manual_receipt.clone()),
    }
}
