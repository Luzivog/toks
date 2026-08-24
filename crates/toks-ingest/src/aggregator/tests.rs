use super::{TokenBreakdown, UnifiedMessage};
use chrono::{DateTime, Utc};

mod daily_aggregation;
mod graph_and_intensity;
mod sessions;
mod summaries_and_years;

fn mock_unified_message(
    date: &str,
    tokens: i64,
    cost: f64,
    model: &str,
    client: &str,
) -> UnifiedMessage {
    let datetime = format!("{}T00:00:00Z", date)
        .parse::<DateTime<Utc>>()
        .unwrap();
    let timestamp = datetime.timestamp_millis();
    UnifiedMessage {
        client: client.to_string(),
        model_id: model.to_string(),
        provider_id: "test-provider".to_string(),
        session_id: "test-session".to_string(),
        workspace_key: None,
        workspace_label: None,
        timestamp,
        date: date.to_string(),
        tokens: TokenBreakdown {
            input: tokens / 2,
            output: tokens / 2,
            cache_read: 0,
            cache_write: 0,
            reasoning: 0,
        },
        cost,
        cost_source: Default::default(),
        duration_ms: None,
        message_count: 1,
        agent: None,
        dedup_key: None,
        durable_identity: None,
        accounting_aliases: Vec::new(),
        session_title: None,
        is_turn_start: false,
        model_attribution_conflicted: false,
    }
}
