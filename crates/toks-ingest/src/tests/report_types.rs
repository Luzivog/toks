use super::*;
#[test]
fn token_breakdown_add_assign_includes_every_field() {
    let mut total = TokenBreakdown {
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
        reasoning: 5,
    };
    total += &TokenBreakdown {
        input: 10,
        output: 20,
        cache_read: 30,
        cache_write: 40,
        reasoning: 50,
    };

    assert_eq!(
        total,
        TokenBreakdown {
            input: 11,
            output: 22,
            cache_read: 33,
            cache_write: 44,
            reasoning: 55,
        }
    );
}

#[test]
fn token_breakdown_add_assign_saturates_each_field() {
    let mut total = TokenBreakdown {
        input: i64::MAX,
        output: i64::MIN,
        cache_read: i64::MAX - 1,
        cache_write: i64::MIN + 1,
        reasoning: 100,
    };
    total += &TokenBreakdown {
        input: 1,
        output: -1,
        cache_read: 10,
        cache_write: -10,
        reasoning: 23,
    };

    assert_eq!(total.input, i64::MAX);
    assert_eq!(total.output, i64::MIN);
    assert_eq!(total.cache_read, i64::MAX);
    assert_eq!(total.cache_write, i64::MIN);
    assert_eq!(total.reasoning, 123);
}

#[test]
fn legacy_monthly_usage_struct_literal_remains_source_compatible() {
    let usage = MonthlyUsage {
        month: "2026-01".to_string(),
        models: vec!["model".to_string()],
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
        message_count: 5,
        cost: 0.5,
    };

    let serialized = serde_json::to_value(usage).unwrap();
    assert!(serialized.get("reasoning").is_none());
}

#[test]
fn monthly_usage_v2_serializes_reasoning_additively() {
    let usage = MonthlyUsageV2 {
        month: "2026-01".to_string(),
        models: vec!["model".to_string()],
        input: 1,
        output: 2,
        cache_read: 3,
        cache_write: 4,
        reasoning: 6,
        message_count: 5,
        cost: 0.5,
    };

    let serialized = serde_json::to_value(&usage).unwrap();
    assert_eq!(serialized["reasoning"], 6);

    let legacy = usage.into_legacy();
    assert_eq!(legacy.month, "2026-01");
    assert_eq!(legacy.models, ["model"]);
    assert_eq!(legacy.input, 1);
    assert_eq!(legacy.output, 2);
    assert_eq!(legacy.cache_read, 3);
    assert_eq!(legacy.cache_write, 4);
    assert_eq!(legacy.message_count, 5);
    assert_eq!(legacy.cost, 0.5);
    assert!(serde_json::to_value(legacy)
        .unwrap()
        .get("reasoning")
        .is_none());
}

#[test]
fn monthly_report_v2_legacy_conversion_preserves_report_metadata() {
    let report = MonthlyReportV2 {
        entries: vec![MonthlyUsageV2 {
            month: "2026-02".to_string(),
            models: vec![],
            input: 1,
            output: 2,
            cache_read: 3,
            cache_write: 4,
            reasoning: 5,
            message_count: 6,
            cost: 0.75,
        }],
        total_cost: 0.75,
        processing_time_ms: 42,
    };

    let legacy = report.into_legacy();
    assert_eq!(legacy.entries.len(), 1);
    assert_eq!(legacy.entries[0].month, "2026-02");
    assert_eq!(legacy.entries[0].input, 1);
    assert_eq!(legacy.entries[0].output, 2);
    assert_eq!(legacy.entries[0].cache_read, 3);
    assert_eq!(legacy.entries[0].cache_write, 4);
    assert_eq!(legacy.entries[0].message_count, 6);
    assert_eq!(legacy.entries[0].cost, 0.75);
    assert_eq!(legacy.total_cost, 0.75);
    assert_eq!(legacy.processing_time_ms, 42);
}

#[test]
fn token_total_saturates_on_overlarge_buckets() {
    // Multiple clamped (i64::MAX) buckets from a corrupt source must
    // saturate rather than overflow when summed.
    let t = TokenBreakdown {
        input: i64::MAX,
        output: i64::MAX,
        cache_read: i64::MAX,
        cache_write: 0,
        reasoning: 0,
    };
    assert_eq!(t.total(), i64::MAX);
    assert_eq!(crate::positive_token_total(&t), i64::MAX);
}

#[test]
fn test_group_by_from_str_valid_values() {
    assert_eq!(GroupBy::from_str("model").unwrap(), GroupBy::Model);
    assert_eq!(
        GroupBy::from_str("client,model").unwrap(),
        GroupBy::ClientModel
    );
    assert_eq!(
        GroupBy::from_str("client-model").unwrap(),
        GroupBy::ClientModel
    );
    assert_eq!(
        GroupBy::from_str("client,provider,model").unwrap(),
        GroupBy::ClientProviderModel
    );
    assert_eq!(
        GroupBy::from_str("client-provider-model").unwrap(),
        GroupBy::ClientProviderModel
    );
    assert_eq!(
        GroupBy::from_str("workspace,model").unwrap(),
        GroupBy::WorkspaceModel
    );
    assert_eq!(
        GroupBy::from_str("workspace-model").unwrap(),
        GroupBy::WorkspaceModel
    );
    assert_eq!(GroupBy::from_str("session").unwrap(), GroupBy::Session);
    assert_eq!(
        GroupBy::from_str("session,model").unwrap(),
        GroupBy::Session
    );
    assert_eq!(
        GroupBy::from_str("session-model").unwrap(),
        GroupBy::Session
    );
    assert_eq!(
        GroupBy::from_str("client,session").unwrap(),
        GroupBy::ClientSession
    );
    assert_eq!(
        GroupBy::from_str("client,session,model").unwrap(),
        GroupBy::ClientSession
    );
    assert_eq!(
        GroupBy::from_str("client-session-model").unwrap(),
        GroupBy::ClientSession
    );
    assert!(GroupBy::from_str("unknown").is_err());
}

#[test]
fn test_group_by_default_is_client_model() {
    assert_eq!(GroupBy::default(), GroupBy::ClientModel);
}

#[test]
fn test_group_by_display_round_trips_with_from_str() {
    let variants = [
        GroupBy::Model,
        GroupBy::ClientModel,
        GroupBy::ClientProviderModel,
        GroupBy::WorkspaceModel,
        GroupBy::Session,
        GroupBy::ClientSession,
    ];

    for variant in variants {
        let rendered = variant.to_string();
        let parsed = GroupBy::from_str(&rendered).unwrap();
        assert_eq!(parsed, variant);
    }
}

#[test]
fn test_group_by_from_str_whitespace_handling() {
    assert_eq!(
        GroupBy::from_str("client, model").unwrap(),
        GroupBy::ClientModel
    );
    assert_eq!(GroupBy::from_str(" model ").unwrap(), GroupBy::Model);
    assert_eq!(
        GroupBy::from_str("client , provider , model").unwrap(),
        GroupBy::ClientProviderModel
    );
    assert_eq!(
        GroupBy::from_str("workspace, model").unwrap(),
        GroupBy::WorkspaceModel
    );
}
