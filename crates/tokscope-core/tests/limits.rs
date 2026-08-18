//! Fixture-driven tests for the dynamic limit-window discovery. The point of
//! these is the *unknown-schema* cases: new windows must render without code
//! changes.

use tokscope_core::limits::{claude, codex, PlanMultiplier, Provider};

#[test]
fn claude_limits_array_is_rendered_dynamically() {
    // Current schema plus a made-up future kind ("monthly_all") and an
    // unknown scope — both must come through as windows.
    let root = serde_json::json!({
        "oauthAccount": {
            "organizationRateLimitTier": "default_claude_max_20x"
        },
        "cachedUsageUtilization": {
            "fetchedAtMs": 1786956888017i64,
            "utilization": {
                "limits": [
                    {"kind": "session", "group": "session", "percent": 6,
                     "severity": "normal", "resets_at": "2026-08-17T09:09:59.9+00:00",
                     "scope": null, "is_active": true},
                    {"kind": "monthly_all", "group": "monthly", "percent": 41,
                     "severity": "warning", "resets_at": "2026-09-01T00:00:00+00:00",
                     "scope": null, "is_active": false},
                    {"kind": "weekly_scoped", "group": "weekly", "percent": 3,
                     "severity": "normal", "resets_at": "2026-08-24T03:59:59.9+00:00",
                     "scope": {"model": {"id": null, "display_name": "Fable"}, "surface": null},
                     "is_active": false}
                ]
            }
        }
    });
    let snap = claude::parse(&root, "test".into()).unwrap();
    assert_eq!(snap.provider, Provider::Claude);
    assert_eq!(snap.plan_multiplier, Some(PlanMultiplier::Twenty));
    assert_eq!(snap.windows.len(), 3);

    let monthly = snap.windows.iter().find(|w| w.id == "monthly_all").unwrap();
    assert_eq!(monthly.label, "Monthly all"); // prettified fallback
    assert_eq!(monthly.percent_used, 41.0);
    assert_eq!(monthly.severity.as_deref(), Some("warning"));

    let scoped = snap
        .windows
        .iter()
        .find(|w| w.id == "weekly_scoped")
        .unwrap();
    assert_eq!(scoped.label, "Weekly — Fable");
    assert_eq!(scoped.scope.as_deref(), Some("Fable"));
}

#[test]
fn claude_structural_fallback_without_limits_array() {
    // Older Claude Code: no limits[]; windows are top-level keyed objects.
    let root = serde_json::json!({
        "cachedUsageUtilization": {
            "fetchedAtMs": 1786956888017i64,
            "utilization": {
                "five_hour": {"utilization": 12, "resets_at": "2026-08-17T09:09:59+00:00"},
                "seven_day": {"utilization": 4, "resets_at": "2026-08-24T03:59:59+00:00"},
                "seven_day_opus": null,
                "some_new_bucket": {"utilization": 55, "resets_at": null},
                "extra_usage": {"is_enabled": false}
            }
        }
    });
    let snap = claude::parse(&root, "test".into()).unwrap();
    let ids: Vec<&str> = snap.windows.iter().map(|w| w.id.as_str()).collect();
    assert!(ids.contains(&"five_hour"));
    assert!(ids.contains(&"seven_day"));
    // Unknown bucket with a utilization + resets_at shape is picked up too.
    assert!(ids.contains(&"some_new_bucket"));
    // extra_usage has no resets_at key → not a window.
    assert!(!ids.contains(&"extra_usage"));
}

#[test]
fn claude_missing_cache_errors_cleanly() {
    let root = serde_json::json!({"somethingElse": 1});
    assert!(claude::parse(&root, "test".into()).is_err());
}

#[test]
fn codex_windows_discovered_structurally() {
    // Weekly-only today; a hypothetical restored 5h window and a brand-new
    // "monthly" object must all be discovered.
    let rl = serde_json::json!({
        "limit_id": "codex",
        "limit_name": null,
        "primary": {"used_percent": 96.0, "window_minutes": 10080, "resets_at": 1787198832},
        "secondary": {"used_percent": 31.5, "window_minutes": 300, "resets_at": 1786999999},
        "monthly": {"used_percent": 12.0, "window_minutes": 43200, "resets_at": 1789999999},
        "credits": {"has_credits": false, "unlimited": false, "balance": "0"},
        "plan_type": "pro",
        "spend_control_reached": null
    });
    let snap = codex::parse(&rl, None, "test".into());
    assert_eq!(snap.provider, Provider::Codex);
    assert_eq!(snap.plan.as_deref(), Some("pro"));
    assert_eq!(snap.plan_multiplier, Some(PlanMultiplier::Twenty));
    assert_eq!(snap.windows.len(), 3);

    let labels: Vec<&str> = snap.windows.iter().map(|w| w.label.as_str()).collect();
    assert!(labels.contains(&"Weekly"));
    assert!(labels.contains(&"5-hour"));
    assert!(labels.contains(&"30-day"));

    // Most-used window is flagged active.
    assert!(snap.windows[0].is_active);
    assert_eq!(snap.windows[0].percent_used, 96.0);

    // Zero-credit credits object is filtered from extras; plan_type stays.
    assert!(!snap.extras.iter().any(|(k, _)| k == "credits"));
}

#[test]
fn codex_prolite_is_the_five_times_pro_product() {
    let response = serde_json::json!({
        "plan_type": "prolite",
        "rate_limit": {
            "primary_window": {
                "used_percent": 10.0,
                "limit_window_seconds": 604800,
                "reset_at": 1787198832
            }
        }
    });
    let snapshot = codex::parse(&response, None, "test".into());
    assert_eq!(snapshot.plan.as_deref(), Some("prolite"));
    assert_eq!(snapshot.plan_multiplier, Some(PlanMultiplier::Five));
}

#[test]
fn codex_array_windows_are_discovered() {
    // If Codex ships e.g. additional_rate_limits: [...], each element with a
    // used_percent becomes a window.
    let rl = serde_json::json!({
        "primary": {"used_percent": 10.0, "window_minutes": 10080, "resets_at": 1787198832},
        "additional_rate_limits": [
            {"used_percent": 77.0, "window_minutes": 1440, "resets_at": 1787000000},
            {"used_percent": 5.0}
        ],
        "plan_type": "plus"
    });
    let snap = codex::parse(&rl, None, "test".into());
    assert_eq!(snap.windows.len(), 3);
    assert!(snap.windows.iter().any(|w| w.label == "1-day"));
    // Element without window_minutes falls back to a prettified key.
    assert!(snap
        .windows
        .iter()
        .any(|w| w.id == "additional_rate_limits[1]"));
}

#[test]
fn codex_wham_usage_response_shape() {
    // The live `wham/usage` endpoint nests windows differently from rollouts:
    // seconds instead of minutes, `reset_at` instead of `resets_at`, and
    // model-scoped limits nested under additional_rate_limits[].rate_limit
    // with a limit_name. All must be discovered, scoped, and labeled.
    let resp = serde_json::json!({
        "email": "user@example.com",
        "plan_type": "pro",
        "rate_limit": {
            "primary_window": {"used_percent": 100.0, "limit_window_seconds": 604800, "reset_at": 1787198832},
            "secondary_window": null
        },
        "additional_rate_limits": [
            {
                "metered_feature": "gpt_5_3_codex_spark",
                "limit_name": "GPT-5.3-Codex-Spark",
                "rate_limit": {
                    "primary_window": {"used_percent": 0.0, "limit_window_seconds": 604800, "reset_at": 1787845258}
                }
            }
        ],
        "credits": {"balance": "0", "has_credits": false, "unlimited": false},
        "spend_control": {"individual_limit": null, "reached": false},
        "rate_limit_reset_credits": {"available_count": 0}
    });
    let snap = codex::parse(&resp, None, "test".into());
    assert_eq!(snap.account.email.as_deref(), Some("user@example.com"));
    assert_eq!(snap.plan.as_deref(), Some("pro"));
    assert_eq!(snap.windows.len(), 2);

    let general = snap.windows.iter().find(|w| w.scope.is_none()).unwrap();
    assert_eq!(general.label, "Weekly");
    assert_eq!(general.percent_used, 100.0);

    let spark = snap.windows.iter().find(|w| w.scope.is_some()).unwrap();
    assert_eq!(spark.label, "Weekly — GPT-5.3-Codex-Spark");
    assert_eq!(spark.percent_used, 0.0);
    assert!(spark.resets_at.is_some());

    // email/zero-credit noise stays out of extras.
    assert!(snap.extras.is_empty(), "extras: {:?}", snap.extras);
}

#[test]
fn reset_elapsed_detection() {
    let rl = serde_json::json!({
        "primary": {"used_percent": 96.0, "window_minutes": 10080, "resets_at": 1000000000}
    });
    let snap = codex::parse(&rl, None, "test".into());
    assert!(snap.windows[0].reset_elapsed(chrono::Utc::now()));
}

#[test]
fn remaining_percent_never_goes_negative() {
    let rl = serde_json::json!({
        "primary": {"used_percent": 140.0, "window_minutes": 300, "resets_at": 1786999999}
    });
    let snap = codex::parse(&rl, None, "test".into());
    assert_eq!(snap.windows[0].percent_remaining(), 0.0);

    let rl = serde_json::json!({
        "primary": {"used_percent": -20.0, "window_minutes": 300, "resets_at": 1786999999}
    });
    let snap = codex::parse(&rl, None, "test".into());
    assert_eq!(snap.windows[0].percent_remaining(), 100.0);
}

#[test]
fn invalid_usage_never_looks_like_full_quota() {
    let rl = serde_json::json!({
        "primary": {"used_percent": 0.0, "window_minutes": 300}
    });
    let mut snap = codex::parse(&rl, None, "test".into());
    snap.windows[0].percent_used = f64::NAN;
    assert_eq!(snap.windows[0].percent_remaining(), 0.0);
}
