//! Provider-neutral account discovery and isolated sign-in.
//!
//! Toks observes provider-owned profiles without copying OAuth grants or
//! switching the user's active CLI account. Managed accounts live in isolated
//! homes where the provider's own CLI remains credential owner.

mod catalog;
mod codex_auth;
mod collection;
mod discovery;
mod lifecycle;
mod login;
mod order;
mod redemption;
mod removal;
mod storage;
mod suppression;
mod types;

pub use catalog::{AccountBinding, AccountTransition};
#[cfg(test)]
pub(crate) use codex_auth::read_for_test as read_codex_auth_for_test;
pub(crate) use codex_auth::{credential_fingerprint, CodexAuthProof, CodexAuthSnapshot};
pub use collection::{collect_limits, hydrate_limits};
pub(crate) use collection::{collect_provider_limits, ProviderLimitCollection};
pub use lifecycle::{
    remove_account, AccountRemovalPlan, AccountRemovalResult, ManagedProfileRemoval,
    ManagedRemovalState,
};
pub use login::{
    begin_add_account, begin_reauthentication, cancel_login, login_outcome, LoginOutcome,
};
pub use order::{apply_saved_order, move_account_to, AccountOrderKey};
pub use redemption::{acknowledge_observed_banked_reset, redeem_banked_reset, BankedResetResult};
pub use removal::remove_from_toks;
pub use suppression::{hide_account, unhide_account, unhide_profile};
pub use types::{
    AccountId, AccountIdentityKind, AccountOrigin, AccountSource, AddAccountStarted,
    CredentialProfileId, CredentialProfileKind, ProviderAccount,
};

pub(crate) use discovery::discover_profiles;
pub(crate) use suppression::filter_hidden_accounts;
pub(crate) use types::AccountProfile;

use catalog::coalesce_snapshots;
#[cfg(test)]
pub(crate) use catalog::codex_auth_account_id_for_test;
pub(crate) use catalog::{codex_auth_account_id, provider_principal_id};
use discovery::account_email;
use storage::{now_millis, now_nanos, profiles_root, write_metadata, PROFILE_VERSION};
use types::ProfileMetadata;

#[cfg(test)]
mod catalog_tests;
#[cfg(test)]
mod collection_auth_tests;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod tests;
