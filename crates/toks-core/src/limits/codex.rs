//! Codex plan limits from local rollout snapshots and the live usage endpoint.
//!
//! Both inputs share one structural parser. Any object with `used_percent`
//! becomes a window, so new provider windows appear without a code update.

mod auth_plan;
#[cfg(test)]
mod auth_plan_tests;
mod local;
mod parser;
mod principal;
mod redeem;
#[cfg(test)]
mod redeem_tests;
mod reset_credits;
#[cfg(test)]
mod reset_credits_tests;

pub use local::read;
pub use parser::parse;

pub(crate) use auth_plan::read_plan_from_auth;
pub(crate) use local::{codex_home, read_email_from_home, read_from_home};
pub(crate) use principal::{
    account_header_matches_auth, principal_material_from_auth, read_principal_material,
};
pub(crate) use redeem::redeem_banked_reset;
pub(crate) use reset_credits::{
    into_domain as reset_credits_into_domain, ResetCreditDetailsResponse,
};
