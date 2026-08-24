//! Session parsers for different AI coding assistant formats
//!
//! Each client has its own parser that converts to a unified message format.

mod accounting_identity;
#[cfg(test)]
mod accounting_identity_tests;
mod agent_names;
pub mod amp;
pub mod antigravity;
pub mod antigravity_cli;
pub mod augment;
pub mod claudecode;
pub mod cline;
pub mod codebuddy;
pub mod codebuff;
pub mod codex;
pub mod commandcode;
#[cfg(test)]
mod commandcode_tests;
pub mod copilot;
pub mod copilot_desktop;
pub mod copilot_vscode;
pub mod crush;
pub mod cursor;
pub mod devin;
pub mod droid;
pub mod freebuff;
pub mod gemini;
pub mod gjc;
pub mod goose;
pub mod grok;
pub mod hermes;
#[cfg(test)]
mod invalid_utf8_tests;
pub mod jcode;
pub mod junie;
pub mod kilo;
pub mod kilocode;
pub mod kimchi;
pub mod kimi;
pub mod kiro;
pub mod micode;
pub mod mux;
pub mod openclaw;
pub mod opencode;
pub mod opencodereview;
pub mod pi;
pub mod prime_agent;
pub mod qwen;
pub mod reasonix;
pub mod roocode;
pub mod senpi;
pub mod synthetic;
pub(crate) mod tencent_buddy;
pub mod trae;
pub(crate) mod utils;
pub mod warp;
pub mod workbuddy;
pub mod zcode;
pub mod zed;

use crate::TokenBreakdown;
pub use accounting_identity::{
    AccountingAlias, AccountingAliasScheme, DurableIdentity, DurableIdentityScheme,
    IdentityStrength,
};
#[cfg(test)]
use agent_names::strip_zero_width_chars;
pub use agent_names::{
    normalize_agent_name, normalize_copilot_agent_name, normalize_opencode_agent_name,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CostSource {
    #[default]
    Unknown,
    ProviderReported,
    Estimated,
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct UnifiedMessage {
    pub client: String,
    pub model_id: String,
    pub provider_id: String,
    pub session_id: String,
    pub workspace_key: Option<String>,
    pub workspace_label: Option<String>,
    pub timestamp: i64,
    pub date: String,
    pub tokens: TokenBreakdown,
    pub cost: f64,
    #[serde(default)]
    pub cost_source: CostSource,
    #[serde(default)]
    pub duration_ms: Option<i64>,
    #[serde(default = "default_message_count")]
    pub message_count: i32,
    pub agent: Option<String>,
    pub dedup_key: Option<String>,
    /// Source-native accounting identity used by Toks's durable archive.
    /// Older source-cache entries deserialize without it.
    #[serde(default)]
    pub durable_identity: Option<DurableIdentity>,
    /// Secondary same-fact hints; never authoritative durable identities.
    #[serde(default)]
    pub accounting_aliases: Vec<AccountingAlias>,
    /// Human-readable session title/name when the source client stores one
    /// (e.g. OpenCode's `session.title` column). `None` for clients that
    /// don't record a title; the Sessions tab falls back to showing just
    /// the session ID in that case.
    #[serde(default)]
    pub session_title: Option<String>,
    /// True if this message is the first assistant response after a user turn.
    /// Used to count user interaction turns (as opposed to API message count).
    #[serde(default)]
    pub is_turn_start: bool,
    /// True when the parser observed conflicting authoritative model evidence.
    /// Such rows must remain unpriced rather than accepting fallback attribution.
    #[serde(default)]
    pub model_attribution_conflicted: bool,
}

const fn default_message_count() -> i32 {
    1
}

impl UnifiedMessage {
    pub fn new(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
    ) -> Self {
        Self::new_full(
            client,
            model_id,
            provider_id,
            session_id,
            timestamp,
            tokens,
            cost,
            None,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_agent(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
        agent: Option<String>,
    ) -> Self {
        Self::new_full(
            client,
            model_id,
            provider_id,
            session_id,
            timestamp,
            tokens,
            cost,
            agent,
            None,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_with_dedup(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
        dedup_key: Option<String>,
    ) -> Self {
        Self::new_full(
            client,
            model_id,
            provider_id,
            session_id,
            timestamp,
            tokens,
            cost,
            None,
            dedup_key,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn new_full(
        client: impl Into<String>,
        model_id: impl Into<String>,
        provider_id: impl Into<String>,
        session_id: impl Into<String>,
        timestamp: i64,
        tokens: TokenBreakdown,
        cost: f64,
        agent: Option<String>,
        dedup_key: Option<String>,
    ) -> Self {
        let date = timestamp_to_date(timestamp);
        Self {
            client: client.into(),
            model_id: model_id.into(),
            provider_id: provider_id.into(),
            session_id: session_id.into(),
            workspace_key: None,
            workspace_label: None,
            timestamp,
            date,
            tokens,
            cost,
            cost_source: CostSource::Unknown,
            duration_ms: None,
            message_count: default_message_count(),
            agent,
            dedup_key,
            durable_identity: None,
            accounting_aliases: Vec::new(),
            session_title: None,
            is_turn_start: false,
            model_attribution_conflicted: false,
        }
    }

    pub fn set_workspace(
        &mut self,
        workspace_key: Option<String>,
        workspace_label: Option<String>,
    ) {
        self.workspace_key = workspace_key;
        self.workspace_label = workspace_label;
    }

    pub(crate) fn refresh_derived_fields(&mut self) {
        self.date = timestamp_to_date(self.timestamp);
    }

    /// Re-derive the day bucket under an explicitly chosen timezone.
    ///
    /// `UnifiedMessage::new` is a constructor called from 92 sites across 42
    /// parser files, so the zone cannot be threaded into it without touching
    /// every one. It does not need to be: `date` is a derived field, already
    /// recomputed from `timestamp` after construction. This lets the one
    /// post-parse pass that holds the user's settings re-key every message at
    /// once, which is the only place the pinned zone is actually known.
    pub(crate) fn rebucket_date(&mut self, timezone: &crate::bucket_tz::BucketTimezone) {
        // A non-positive timestamp is the parsers' "no usable time" sentinel,
        // not an instant before 1970. Re-keying it would move garbage between
        // two equally wrong days, and it is also what bounds the window the
        // auto-pin agreement check has to cover: leaving these alone is what
        // makes `AGREEMENT_WINDOW_START_MS` a real lower bound rather than a
        // convenient one.
        if self.timestamp <= 0 {
            return;
        }

        let key = timezone.day_key(self.timestamp);
        // An unrepresentable instant yields an empty key. Keeping the previous
        // date is wrong by at most the offset between two zones; replacing it
        // with `""` would collapse the message into a bucket that is not a day
        // at all, and that bucket would then be submitted.
        if !key.is_empty() {
            self.date = key;
        }
    }

    pub(crate) fn set_timestamp(&mut self, timestamp: i64) {
        self.timestamp = timestamp;
        self.refresh_derived_fields();
    }

    pub fn mark_provider_reported_cost(&mut self) {
        self.cost_source = CostSource::ProviderReported;
    }

    pub(crate) fn mark_estimated_cost(&mut self) {
        self.cost_source = CostSource::Estimated;
    }

    pub(crate) fn has_authoritative_cost(&self) -> bool {
        self.cost_source == CostSource::ProviderReported
    }
}

pub fn normalize_workspace_key(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }

    let preserve_unc_prefix = trimmed.starts_with("\\\\") || trimmed.starts_with("//");
    let mut normalized = trimmed.replace('\\', "/");

    if preserve_unc_prefix {
        let body = normalized.trim_start_matches('/');
        let mut collapsed = body.to_string();
        while collapsed.contains("//") {
            collapsed = collapsed.replace("//", "/");
        }
        normalized = format!("//{}", collapsed);
    } else {
        while normalized.contains("//") {
            normalized = normalized.replace("//", "/");
        }
    }

    let minimum_len = if preserve_unc_prefix { 2 } else { 1 };
    if normalized.len() > minimum_len {
        normalized = normalized.trim_end_matches('/').to_string();
    }

    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

pub fn workspace_label_from_key(key: &str) -> Option<String> {
    key.rsplit('/')
        .find(|segment| !segment.is_empty())
        .map(|segment| segment.to_string())
}

/// Convert Unix milliseconds to a local YYYY-MM-DD date string.
fn timestamp_to_date(timestamp_ms: i64) -> String {
    timestamp_to_date_with_timezone(timestamp_ms, &chrono::Local)
}

fn timestamp_to_date_with_timezone<Tz>(timestamp_ms: i64, timezone: &Tz) -> String
where
    Tz: chrono::TimeZone,
    Tz::Offset: std::fmt::Display,
{
    crate::bucket_tz::format_day_key(timestamp_ms, timezone)
}

#[cfg(test)]
mod tests;
