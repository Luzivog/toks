//! Persisted planning for zero-disconnect router generation handoffs.

mod model;
mod process;
mod retry;
mod state;

pub(crate) use model::RetryId;
pub use model::{BuildId, DeployPlan, DeploymentEvent, GenerationId, GenerationStatus};
pub(crate) use process::{run_coordinator, run_worker};
pub(crate) use retry::{clear_retry_intent, load_retry_intent, request_retry, RetryIntent};
pub use state::DeploymentState;

pub(in crate::codex_router) const COORDINATOR_PRE_SIGNAL_OPERATION_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(7);
pub(in crate::codex_router) const COORDINATOR_SHUTDOWN_DRAIN_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(10);

#[cfg(test)]
mod tests;
