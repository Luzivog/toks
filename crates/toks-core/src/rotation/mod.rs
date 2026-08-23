//! Account-selection state shared by the Toks UI and local Codex router.
//!
//! Settings and runtime data have separate stores. The UI owns settings while
//! the router owns runtime data, so the two processes never write one file.

mod quota;
mod runtime;
mod settings;
mod storage;
mod types;

pub use quota::{account_quota_drain, AccountQuotaDrain};
pub use runtime::{
    AccountAvailability, AccountRuntime, RotationRuntime, RouterHealth, WaitingThread,
};
pub use settings::RotationSettings;
pub(crate) use storage::write_private_atomic;
pub use storage::{RotationPaths, RotationRuntimeStore, RotationSettingsStore};
pub use types::{BlockWindow, RotationEvent, RotationEventKind, ThreadId, UnixMillis};
pub(crate) use types::{FastLimitDisposition, FastLimitOutcome};

#[cfg(test)]
mod runtime_limit_tests;
#[cfg(test)]
mod tests;
