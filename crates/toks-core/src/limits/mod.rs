//! Plan-limit snapshots for supported providers.
//!
//! Providers discover limit windows from source data and map them into the
//! generic [`LimitWindow`], so new provider windows need no code update.

pub mod claude;
mod claude_auth;
mod claude_credentials;
mod claude_lock;
pub mod codex;
mod credentials;
mod http;
pub mod live;
mod live_fetch;
mod plan;
mod reset_credits;
pub(crate) mod settling;
mod snapshot_cache;
mod status;

#[cfg(test)]
mod claude_auth_tests;
#[cfg(test)]
mod live_tests;
#[cfg(test)]
mod settling_tests;
#[cfg(test)]
mod snapshot_cache_tests;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub use plan::PlanMultiplier;
pub(crate) use plan::{read_claude_plan, PlanDetails};
pub use reset_credits::{
    BankedResetAttempt, BankedResetCredit, BankedResetCreditStatus, BankedResetOutcome,
};
pub use status::{LimitIssue, LimitIssueKind, SnapshotFreshness, SnapshotStatus};

pub(crate) fn forget_account_profile(
    provider: Provider,
    profile_id: &crate::accounts::CredentialProfileId,
) {
    live::forget_profile(provider, profile_id);
    let _ = snapshot_cache::remove_for_profile(provider, profile_id);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Claude,
    Codex,
}

impl Provider {
    pub const ALL: [Provider; 2] = [Provider::Claude, Provider::Codex];

    pub fn display_name(&self) -> &'static str {
        match self {
            Provider::Claude => "Claude Code",
            Provider::Codex => "Codex",
        }
    }

    pub fn slug(&self) -> &'static str {
        match self {
            Provider::Claude => "claude",
            Provider::Codex => "codex",
        }
    }
}

/// One usage window (e.g. "Session", "Weekly", "Weekly — Fable").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitWindow {
    /// Stable identifier from the source data (kind or JSON key).
    pub id: String,
    /// Human label, best-effort; falls back to a prettified `id`.
    pub label: String,
    /// 0–100.
    pub percent_used: f64,
    pub resets_at: Option<DateTime<Utc>>,
    /// Source-provided severity ("normal", "warning", …) when present.
    pub severity: Option<String>,
    /// Scope qualifier (e.g. a model display name) when present.
    pub scope: Option<String>,
    /// Whether the source marks this window as the currently-binding one.
    pub is_active: bool,
    /// The full source object, for tooltips/debugging.
    pub raw: serde_json::Value,
}

impl LimitWindow {
    /// True when the window's reset time has passed since the snapshot was
    /// written — `percent_used` then describes the *previous* window.
    pub fn reset_elapsed(&self, now: DateTime<Utc>) -> bool {
        self.resets_at.map(|r| r < now).unwrap_or(false)
    }

    /// Provider APIs report usage; the product shows the more useful inverse.
    pub fn percent_remaining(&self) -> f64 {
        let used = if self.percent_used.is_finite() {
            self.percent_used
        } else {
            // Invalid provider values must never look like a healthy quota.
            // Parsers normally sanitize this at ingress; this conservative
            // fallback keeps manually-constructed and migrated snapshots safe.
            100.0
        };
        (100.0 - used).clamp(0.0, 100.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LimitSnapshot {
    pub provider: Provider,
    /// Opaque logical account identity plus its exact local credential sources.
    pub account: crate::accounts::ProviderAccount,
    /// Plan name as reported by the source ("max", "pro", …).
    pub plan: Option<String>,
    /// Provider-reported allowance multiplier. Never inferred from `plan`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_multiplier: Option<PlanMultiplier>,
    /// Redeemable Codex credits that reset both standard usage windows.
    #[serde(default)]
    pub banked_resets: u64,
    /// Optional returned rows; the provider count above remains authoritative.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub banked_reset_credits: Option<Vec<BankedResetCredit>>,
    pub windows: Vec<LimitWindow>,
    /// Non-window facts worth surfacing (credits, spend, extra usage…).
    pub extras: Vec<(String, serde_json::Value)>,
    /// When the source data was produced (drives the staleness badge).
    pub fetched_at: Option<DateTime<Utc>>,
    /// Where the data came from (file path), for diagnostics.
    pub source: String,
    /// Account-local collection problem. Other accounts remain available.
    pub issue: Option<String>,
    /// Typed freshness and refresh state. `source` and `issue` remain for
    /// compatibility with older callers and cache envelopes.
    #[serde(default)]
    pub status: SnapshotStatus,
}

impl LimitSnapshot {
    /// Create the immediate placeholder for a newly-started provider sign-in.
    /// The account collection pass will replace it with cached or live data.
    pub fn loading_account(provider: Provider, account: crate::accounts::ProviderAccount) -> Self {
        Self {
            provider,
            account,
            plan: None,
            plan_multiplier: None,
            banked_resets: 0,
            banked_reset_credits: None,
            windows: Vec::new(),
            extras: Vec::new(),
            fetched_at: None,
            source: String::new(),
            issue: None,
            status: SnapshotStatus::at(SnapshotFreshness::Loading),
        }
    }

    /// True only while an authenticated account has no successful local or
    /// live snapshot yet. This is a loading state, not an error state.
    pub fn is_pending(&self) -> bool {
        self.status.freshness == SnapshotFreshness::Loading
    }
}

/// Prettify a raw identifier: `weekly_all` → `Weekly all`.
fn humanize_id(id: &str) -> String {
    let s = id.replace(['_', '-'], " ");
    let mut chars = s.chars();
    match chars.next() {
        Some(f) => f.to_uppercase().collect::<String>() + chars.as_str(),
        None => s,
    }
}

fn parse_rfc3339(v: &serde_json::Value) -> Option<DateTime<Utc>> {
    v.as_str()
        .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
        .map(|d| d.with_timezone(&Utc))
}

/// Read every discovered account from the live provider endpoints (throttled,
/// see [`live`]). Successful reads replace Toks's per-account snapshot;
/// the last snapshot remains available when a provider is temporarily down.
pub fn collect_all() -> Vec<LimitSnapshot> {
    crate::accounts::collect_limits()
}

/// Read Toks-owned and provider-owned snapshots without making a network
/// request. Apps should publish this result before starting a refresh.
pub fn hydrate_all() -> Vec<LimitSnapshot> {
    crate::accounts::hydrate_limits()
}
