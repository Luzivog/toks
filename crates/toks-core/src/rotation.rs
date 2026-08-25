//! Account-selection state shared by the Toks UI and local Codex router.
//!
//! Settings and runtime data have separate stores. The UI owns settings;
//! router generations serialize runtime transactions through the store lock.

mod quota;
#[cfg(test)]
mod quota_tests;
mod runtime;
mod settings;
mod storage;
mod types;

pub(crate) use quota::QuotaObservation;
pub use quota::{account_quota_drain, AccountQuotaDrain};
pub(crate) use runtime::ThreadOwnership;
pub use runtime::{
    AccountAvailability, AccountRuntime, RotationRuntime, RouterHealth, ThreadAccountConflict,
    ThreadRequestSettings, ThreadRow, ThreadStatus, WaitingId, WaitingThread,
};
pub(crate) use runtime::{ResumeAuthorization, ResumeRoute, ResumeTerminal, WorkerConnectionOwner};
pub use settings::{
    InvalidThreadOverrideValue, RotationSettings, ThreadOverride, ThreadOverrideChange,
};
pub use storage::{RotationPaths, RotationRuntimeStore, RotationSettingsStore};
pub use types::{
    BlockWindow, RotationEvent, RotationEventKind, ThreadId, UnixMillis, UsageLimitClassification,
    UsageLimitEvidence, UsageLimitIncident, UsageLimitPhase, UsageLimitTier, UsageLimitTierOrigin,
};
pub(crate) use types::{FastLimitDisposition, FastLimitOutcome};

#[cfg(test)]
mod runtime_active_thread_tests;
#[cfg(test)]
mod runtime_limit_tests;
#[cfg(test)]
mod runtime_quota_observation_tests;
#[cfg(test)]
mod runtime_reconciliation_tests;
#[cfg(test)]
mod runtime_resume_validation_tests;
#[cfg(test)]
mod runtime_settings_queue_tests;
#[cfg(test)]
mod runtime_unknown_reset_tests;
#[cfg(test)]
mod tests;
