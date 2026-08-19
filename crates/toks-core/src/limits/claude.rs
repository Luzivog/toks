//! Claude Code plan limits from its local cache and live usage response. Both
//! shapes feed [`parse_utilization`].

mod principal;

pub(crate) use principal::read_principal_material;

use super::{
    humanize_id, parse_rfc3339, read_claude_plan, LimitSnapshot, LimitWindow, PlanMultiplier,
    Provider,
};
use anyhow::{Context, Result};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::path::{Path, PathBuf};

pub fn read() -> Result<LimitSnapshot> {
    let home = dirs::home_dir().context("no home dir")?;
    let config_dir = std::env::var("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".claude"));
    read_from_profile(&home, &config_dir)
}

/// Read Claude's provider-owned local cache from one explicit profile.
pub(crate) fn read_from_profile(home: &Path, config_dir: &Path) -> Result<LimitSnapshot> {
    let path = claude_json_path(home, config_dir);
    let raw =
        std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let root: Value = serde_json::from_str(&raw).context("parsing ~/.claude.json")?;
    let mut snapshot = parse(&root, path.display().to_string())?;
    let details = read_claude_plan(config_dir);
    snapshot.plan = details.name;
    snapshot.plan_multiplier = details.multiplier.or(snapshot.plan_multiplier);
    snapshot.account.email = email_from_root(&root);
    Ok(snapshot)
}

fn claude_json_path(home: &Path, config_dir: &Path) -> PathBuf {
    // Relocated profiles may keep the cache in the config directory. Native
    // profiles keep it at the root of HOME.
    let relocated = config_dir.join(".claude.json");
    if relocated.exists() {
        relocated
    } else {
        home.join(".claude.json")
    }
}

pub(crate) fn read_email_from_profile(home: &Path, config_dir: &Path) -> Option<String> {
    let raw = std::fs::read_to_string(claude_json_path(home, config_dir)).ok()?;
    let root: Value = serde_json::from_str(&raw).ok()?;
    email_from_root(&root)
}

fn email_from_root(root: &Value) -> Option<String> {
    root.pointer("/oauthAccount/emailAddress")
        .and_then(Value::as_str)
        .filter(|email| !email.is_empty())
        .map(str::to_string)
}

pub fn parse(root: &Value, source: String) -> Result<LimitSnapshot> {
    let cached = root
        .get("cachedUsageUtilization")
        .context("no cachedUsageUtilization in .claude.json (run Claude Code once)")?;
    let fetched_at = cached
        .get("fetchedAtMs")
        .and_then(Value::as_i64)
        .and_then(|ms| Utc.timestamp_millis_opt(ms).single());
    let util = cached
        .get("utilization")
        .context("no utilization payload")?;
    let mut snapshot = parse_utilization(util, fetched_at, source);
    snapshot.plan_multiplier =
        PlanMultiplier::from_explicit_metadata(root).or(snapshot.plan_multiplier);
    snapshot.account.email = email_from_root(root);
    Ok(snapshot)
}

/// Build a snapshot from a `/api/oauth/usage` response body (which is exactly
/// what `cachedUsageUtilization.utilization` caches verbatim).
pub fn parse_utilization(
    util: &Value,
    fetched_at: Option<DateTime<Utc>>,
    source: String,
) -> LimitSnapshot {
    let mut windows = windows_from_limits_array(util);
    if windows.is_empty() {
        windows = windows_from_structural_scan(util);
    }

    let extras = [("spend", "enabled"), ("extra_usage", "is_enabled")]
        .into_iter()
        .filter_map(|(name, enabled)| {
            let value = util.get(name)?;
            (value.get(enabled).and_then(Value::as_bool) == Some(true))
                .then(|| (name.to_string(), value.clone()))
        })
        .collect();

    LimitSnapshot {
        provider: Provider::Claude,
        account: crate::accounts::ProviderAccount::unidentified_for(Provider::Claude),
        plan: None,
        plan_multiplier: PlanMultiplier::from_explicit_metadata(util),
        windows,
        extras,
        fetched_at,
        source,
        issue: None,
        status: Default::default(),
    }
}

/// Primary path: the normalized `limits[]` array. Every entry is rendered —
/// unknown kinds/groups/scopes included — so schema growth is automatic.
fn windows_from_limits_array(util: &Value) -> Vec<LimitWindow> {
    let Some(limits) = util.get("limits").and_then(Value::as_array) else {
        return Vec::new();
    };
    limits
        .iter()
        .filter_map(|l| {
            let percent = l.get("percent").and_then(Value::as_f64)?;
            let kind = l
                .get("kind")
                .and_then(Value::as_str)
                .unwrap_or("limit")
                .to_string();
            let scope = l
                .pointer("/scope/model/display_name")
                .and_then(Value::as_str)
                .map(str::to_string)
                .or_else(|| {
                    l.pointer("/scope/surface")
                        .and_then(Value::as_str)
                        .map(str::to_string)
                });
            Some(LimitWindow {
                label: label_for_kind(&kind, scope.as_deref()),
                id: kind,
                percent_used: percent,
                resets_at: l.get("resets_at").and_then(parse_rfc3339),
                severity: l
                    .get("severity")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                scope,
                is_active: l.get("is_active").and_then(Value::as_bool).unwrap_or(false),
                raw: l.clone(),
            })
        })
        .collect()
}

/// Fallback for older Claude Code versions without `limits[]`: any object
/// under `utilization` carrying a numeric `utilization` plus a `resets_at`
/// key is treated as a window, keyed by its JSON name.
fn windows_from_structural_scan(util: &Value) -> Vec<LimitWindow> {
    let Some(map) = util.as_object() else {
        return Vec::new();
    };
    map.iter()
        .filter_map(|(key, v)| {
            let obj = v.as_object()?;
            let percent = obj.get("utilization")?.as_f64()?;
            if !obj.contains_key("resets_at") {
                return None;
            }
            Some(LimitWindow {
                id: key.clone(),
                label: humanize_id(key),
                percent_used: percent,
                resets_at: obj.get("resets_at").and_then(parse_rfc3339),
                severity: None,
                scope: None,
                is_active: false,
                raw: v.clone(),
            })
        })
        .collect()
}

/// Friendly names for the kinds observed today; anything new falls back to a
/// prettified raw kind so it still renders.
fn label_for_kind(kind: &str, scope: Option<&str>) -> String {
    let base = match kind {
        "session" => "Session".to_string(),
        "weekly_all" => "Weekly (all models)".to_string(),
        "weekly_scoped" => "Weekly".to_string(),
        other => humanize_id(other),
    };
    match scope {
        Some(s) => format!("{base} — {s}"),
        None => base,
    }
}
