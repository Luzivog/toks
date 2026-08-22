//! Account-selection state shared by the Toks UI and local Codex router.
//!
//! Settings and runtime data have separate stores. The UI owns settings while
//! the router owns runtime data, so the two processes never write one file.

mod runtime;
mod settings;
mod storage;
mod types;

pub use runtime::{AccountRuntime, RotationRuntime, RouterHealth, WaitingThread};
pub use settings::RotationSettings;
pub(crate) use storage::write_private_atomic;
pub use storage::{RotationPaths, RotationRuntimeStore, RotationSettingsStore};
pub use types::{RotationEvent, RotationEventKind, ThreadId, UnixMillis};

#[cfg(test)]
mod tests;
