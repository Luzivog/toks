//! Exact-account, one-shot Codex tasks used to start fresh weekly windows.

use anyhow::Result;
use chrono::{DateTime, Utc};

use crate::accounts::{AccountId, ProviderLimitCollection};

mod authority;
mod catalogue;
mod command;
mod job;
mod model;
mod owner;
mod planner;
mod requests;
mod status;
mod store;

use model::FailureReason;
pub use status::{AccountActivationStatus, AutomaticTestStatus, ManualTestStatus};
use store::Store;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManualRequest {
    Queued,
    AlreadyRunning,
}

/// Queues one exact `test` prompt for the selected account. Repeated calls
/// while that account already has a manual task do not create another task.
pub fn request_test(account: &AccountId) -> Result<ManualRequest> {
    let now_ms = Utc::now().timestamp_millis();
    let owner = owner::ProcessOwner::current()
        .ok_or_else(|| anyhow::anyhow!("could not identify the activation worker process"))?;
    let result = Store::discover()?.update(|document| {
        let result = requests::manual(document, account, owner, now_ms);
        let changed = result == ManualRequest::Queued;
        (result, changed)
    })?;
    if result == ManualRequest::Queued {
        let account = account.clone();
        let worker_account = account.clone();
        if let Err(error) = std::thread::Builder::new()
            .name("toks-account-test".into())
            .spawn(move || run_manual_pass(worker_account))
        {
            mark_pending_failed(&account, FailureReason::SpawnFailed);
            return Err(error.into());
        }
    }
    Ok(result)
}

/// Automatic weekly activation is enabled unless an account is opted out.
pub fn set_automatic(account: &AccountId, enabled: bool) -> Result<()> {
    Store::discover()?.update(|document| {
        let before = document.clone();
        requests::set_automatic(document, account, enabled);
        let changed = *document != before;
        ((), changed)
    })
}

pub fn status(account: &AccountId) -> Result<AccountActivationStatus> {
    let now_ms = Utc::now().timestamp_millis();
    Store::discover()?.update(|document| {
        let before = document.clone();
        requests::reconcile_account(document, account, now_ms);
        let status = requests::status(document, account);
        (status, *document != before)
    })
}

pub(crate) fn observe_and_launch(
    collection: &ProviderLimitCollection,
    observed_at: DateTime<Utc>,
) -> Result<()> {
    let now_ms = observed_at.timestamp_millis();
    let launches = claim(collection, now_ms, None)?;
    for launch in launches {
        tokio::spawn(execute_and_record(launch));
    }
    Ok(())
}

fn run_manual_pass(account: AccountId) {
    let collection = crate::accounts::collect_provider_limits(crate::limits::Provider::Codex);
    let now_ms = Utc::now().timestamp_millis();
    let launches = claim(&collection, now_ms, Some(&account)).unwrap_or_default();
    let Some(launch) = launches
        .into_iter()
        .find(|launch| launch.account == account)
    else {
        let _ = Store::discover().and_then(|store| {
            store.update(|document| {
                let changed = planner::fail_pending_manual(
                    document,
                    &account,
                    FailureReason::ProfileUnavailable,
                    now_ms,
                );
                ((), changed)
            })
        });
        return;
    };
    match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime.block_on(execute_and_record(launch)),
        Err(_) => {
            let _ = record_outcome(
                &launch.id,
                std::result::Result::Err(FailureReason::SpawnFailed),
            );
        }
    }
}

fn mark_pending_failed(account: &AccountId, reason: FailureReason) {
    let now_ms = Utc::now().timestamp_millis();
    let _ = Store::discover().and_then(|store| {
        store.update(|document| {
            let changed = planner::fail_pending_manual(document, account, reason, now_ms);
            ((), changed)
        })
    });
}

fn claim(
    collection: &ProviderLimitCollection,
    now_ms: i64,
    only: Option<&AccountId>,
) -> Result<Vec<model::Launch>> {
    let mut authorities = authority::proved(collection, now_ms);
    authorities.retain(|authority| only.is_none_or(|account| &authority.account == account));
    Store::discover()?.update(|document| {
        let before = document.clone();
        let launches = planner::observe(document, &authorities, now_ms);
        let changed = *document != before;
        (launches, changed)
    })
}

async fn execute_and_record(launch: model::Launch) {
    let id = launch.id.clone();
    let result = command::run(&launch).await;
    if let Err(error) = record_outcome(&id, result) {
        eprintln!("toks account activation outcome could not be recorded: {error:#}");
    }
}

fn record_outcome(id: &str, result: std::result::Result<(), FailureReason>) -> Result<()> {
    let now_ms = Utc::now().timestamp_millis();
    let (success, reason) = match result {
        Ok(()) => (true, FailureReason::Unsuccessful),
        Err(reason) => (false, reason),
    };
    Store::discover()?.update(|document| {
        let changed = planner::finish(document, id, success, reason, now_ms);
        ((), changed)
    })
}

#[cfg(test)]
mod tests;
