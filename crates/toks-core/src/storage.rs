//! Shared private-file persistence and locking.

mod atomic;
mod lock;
mod update;

pub(crate) use atomic::{
    restrict_directory, unique_temp_path, write_private_atomic, write_private_atomic_capped,
};
pub(crate) use lock::{lock_private, LockMode, PrivateFileLock};
pub use update::StoreUpdate;
