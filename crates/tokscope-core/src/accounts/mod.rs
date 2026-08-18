//! Provider-neutral account discovery and isolated sign-in.
//!
//! Tokscope observes provider-owned profiles without copying OAuth grants or
//! switching the user's active CLI account. Managed accounts live in isolated
//! homes where the provider's own CLI remains credential owner.

mod collection;
mod discovery;
mod login;
mod order;
mod storage;
mod types;

pub use collection::{collect_limits, hydrate_limits};
pub use login::begin_add_account;
pub use order::{apply_saved_order, move_account_to, AccountOrderKey};
pub use types::{AddAccountStarted, ProviderAccount};

pub(crate) use discovery::discover_profiles;
pub(crate) use types::AccountProfile;

use discovery::account_email;
use storage::{
    now_millis, now_nanos, profiles_root, restrict_directory, write_metadata, PROFILE_VERSION,
};
use types::ProfileMetadata;

#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod tests;
