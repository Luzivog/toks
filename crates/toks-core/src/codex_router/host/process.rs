//! Coordinator and worker process lifecycle for zero-disconnect router reloads.

mod activated;
mod channel;
mod coordinator;
mod coordinator_identity;
mod paths;
mod worker;

pub(crate) use coordinator::run as run_coordinator;
pub(crate) use worker::run as run_worker;

#[cfg(test)]
mod coordinator_identity_tests;
#[cfg(test)]
mod deployment_failure_tests;
#[cfg(test)]
mod test_fixtures;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod worker_identity_tests;
#[cfg(test)]
mod worker_recovery_tests;
