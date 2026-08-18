//! Codex plan limits from local rollout snapshots and the live usage endpoint.
//!
//! Both inputs share one structural parser. Any object with `used_percent`
//! becomes a window, so new provider windows appear without a code update.

mod auth_plan;
mod local;
mod parser;

pub use local::read;
pub use parser::parse;

pub(crate) use auth_plan::read_plan_from_auth;
pub(crate) use local::{codex_home, read_email_from_home, read_from_home};
