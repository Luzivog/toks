use super::live_fetch::set_reset_credit_details;

#[test]
fn missing_optional_details_preserve_the_usage_count() {
    let mut snapshot = snapshot();
    set_reset_credit_details(&mut snapshot, None);
    assert_eq!(snapshot.banked_resets, 3);
    assert_eq!(snapshot.banked_reset_credits, None);
}

#[test]
fn detail_endpoint_count_cannot_override_the_usage_count() {
    let mut snapshot = snapshot();
    let details = serde_json::from_value(serde_json::json!({
        "available_count": 99,
        "credits": [{"status": "available"}]
    }))
    .unwrap();
    set_reset_credit_details(&mut snapshot, Some(details));
    assert_eq!(snapshot.banked_resets, 3);
    assert_eq!(snapshot.banked_reset_credits.unwrap().len(), 1);
}

fn snapshot() -> crate::limits::LimitSnapshot {
    crate::limits::codex::parse(
        &serde_json::json!({
            "rate_limit": {"primary_window": {
                "used_percent": 50,
                "limit_window_seconds": 3600
            }},
            "rate_limit_reset_credits": {"available_count": 3}
        }),
        None,
        "test".into(),
    )
}
