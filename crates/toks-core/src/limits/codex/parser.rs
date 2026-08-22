use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;

use crate::limits::{humanize_id, LimitSnapshot, LimitWindow, PlanMultiplier, Provider};

pub fn parse(
    rate_limits: &Value,
    fetched_at: Option<DateTime<Utc>>,
    source: String,
) -> LimitSnapshot {
    let mut windows = Vec::new();
    let mut extras = Vec::new();

    if let Some(map) = rate_limits.as_object() {
        for (key, v) in map {
            let found_before = windows.len();
            collect_windows(key, v, None, &mut windows);
            let found_any = windows.len() > found_before;
            if !found_any && !v.is_null() && !v.is_object() && !v.is_array() {
                if !is_boring_extra(key, v) {
                    extras.push((key.clone(), v.clone()));
                }
            } else if !found_any && (v.is_object() || v.is_array()) && !is_boring_extra(key, v) {
                extras.push((key.clone(), v.clone()));
            }
        }
    }

    // Most-binding window first.
    windows.sort_by(|a, b| {
        b.percent_used
            .partial_cmp(&a.percent_used)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(first) = windows.first_mut() {
        first.is_active = true;
    }

    let plan = rate_limits
        .get("plan_type")
        .and_then(Value::as_str)
        .map(str::to_string);
    let plan_multiplier = plan
        .as_deref()
        .and_then(PlanMultiplier::from_codex_plan_type)
        .or_else(|| PlanMultiplier::from_explicit_metadata(rate_limits));
    let banked_resets = rate_limits
        .pointer("/rate_limit_reset_credits/available_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);

    LimitSnapshot {
        provider: Provider::Codex,
        account: crate::accounts::ProviderAccount {
            email: rate_limits
                .get("email")
                .and_then(Value::as_str)
                .filter(|email| !email.is_empty())
                .map(str::to_string),
            ..crate::accounts::ProviderAccount::unidentified_for(Provider::Codex)
        },
        plan,
        plan_multiplier,
        banked_resets,
        banked_reset_credits: None,
        windows,
        extras,
        fetched_at,
        source,
        issue: None,
        status: Default::default(),
    }
}

/// Recursive window discovery. An object with `used_percent` is a window;
/// other objects are descended into, and if they carry a `limit_name` or
/// `metered_feature` that string scopes every window found beneath them.
fn collect_windows(key: &str, v: &Value, scope: Option<&str>, out: &mut Vec<LimitWindow>) {
    match v {
        Value::Object(obj) => {
            if obj.contains_key("used_percent") {
                if let Some(w) = window_from_object(key, v, scope) {
                    out.push(w);
                }
                return;
            }
            let own_scope = obj
                .get("limit_name")
                .or_else(|| obj.get("metered_feature"))
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty());
            let scope = own_scope.or(scope);
            for (k, child) in obj {
                if child.is_object() || child.is_array() {
                    collect_windows(k, child, scope, out);
                }
            }
        }
        Value::Array(arr) => {
            for (i, item) in arr.iter().enumerate() {
                collect_windows(&format!("{key}[{i}]"), item, scope, out);
            }
        }
        _ => {}
    }
}

fn window_from_object(key: &str, v: &Value, scope: Option<&str>) -> Option<LimitWindow> {
    let obj = v.as_object()?;
    let percent = obj.get("used_percent")?.as_f64()?;
    // Epoch seconds under either name (rollouts: resets_at, wham: reset_at).
    let resets_at = obj
        .get("resets_at")
        .or_else(|| obj.get("reset_at"))
        .and_then(|r| r.as_i64().or_else(|| r.as_f64().map(|f| f as i64)))
        .and_then(|s| Utc.timestamp_opt(s, 0).single());
    // Window duration under either name (minutes or seconds).
    let minutes = obj
        .get("window_minutes")
        .and_then(Value::as_i64)
        .or_else(|| {
            obj.get("limit_window_seconds")
                .and_then(Value::as_i64)
                .map(|s| s / 60)
        });
    let base = minutes
        .map(label_for_minutes)
        .unwrap_or_else(|| humanize_id(key));
    let label = match scope {
        Some(s) => format!("{base} — {s}"),
        None => base,
    };
    Some(LimitWindow {
        id: match scope {
            Some(s) => format!("{key}:{s}"),
            None => key.to_string(),
        },
        label,
        percent_used: percent,
        resets_at,
        severity: None,
        scope: scope.map(str::to_string),
        is_active: false,
        raw: v.clone(),
    })
}

/// Label a window by its duration, whatever it is: 300 → "5-hour",
/// 10080 → "Weekly", 43200 → "30-day".
fn label_for_minutes(minutes: i64) -> String {
    match minutes {
        m if m <= 0 => "Window".to_string(),
        10080 => "Weekly".to_string(),
        m if m % 1440 == 0 => format!("{}-day", m / 1440),
        m if m % 60 == 0 => format!("{}-hour", m / 60),
        m => format!("{m}-minute"),
    }
}

/// Suppress noise-only fields from extras (nulls handled by caller).
fn is_boring_extra(key: &str, v: &Value) -> bool {
    match key {
        // plan_type is surfaced as the snapshot's plan, not an extra; ids and
        // upsell prompts are noise.
        "limit_id"
        | "limit_name"
        | "metered_feature"
        | "email"
        | "account"
        | "plan_type"
        | "account_id"
        | "user_id"
        | "rate_limit_upsell"
        | "rate_limit_reached_type"
        | "rate_limit_reset_credits" => true,
        "credits" => {
            // Only interesting when the account actually has credits.
            v.pointer("/has_credits").and_then(Value::as_bool) != Some(true)
                && v.pointer("/unlimited").and_then(Value::as_bool) != Some(true)
        }
        "spend_control" => v.pointer("/reached").and_then(Value::as_bool) != Some(true),
        _ => false,
    }
}
