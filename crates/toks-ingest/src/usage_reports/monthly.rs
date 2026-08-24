use crate::{
    filter_messages_for_report, get_home_dir_string, load_pricing_for_local_parse,
    model_name_for_grouping, parse_all_messages_with_pricing_with_env_strategy, ClientId,
    MonthlyReport, MonthlyReportV2, MonthlyUsageV2, ReportOptions, UnifiedMessage,
};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Default)]
struct MonthAggregator {
    models: HashSet<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    message_count: i32,
    cost: f64,
}

pub(crate) fn aggregate_monthly_usage_v2_entries(
    messages: impl IntoIterator<Item = UnifiedMessage>,
) -> Vec<MonthlyUsageV2> {
    let mut month_map: HashMap<String, MonthAggregator> = HashMap::new();

    for msg in messages {
        let Ok(date) = chrono::NaiveDate::parse_from_str(&msg.date, "%Y-%m-%d") else {
            continue;
        };
        let month = date.format("%Y-%m").to_string();

        let entry = month_map.entry(month).or_default();

        entry.models.insert(model_name_for_grouping(
            &msg.client,
            &msg.provider_id,
            &msg.model_id,
        ));
        // Saturating arithmetic matches the model/hourly aggregators: parser
        // clamps can legitimately produce i64::MAX, and a corrupt source must
        // not make report generation overflow.
        entry.input = entry.input.saturating_add(msg.tokens.input);
        entry.output = entry.output.saturating_add(msg.tokens.output);
        entry.cache_read = entry.cache_read.saturating_add(msg.tokens.cache_read);
        entry.cache_write = entry.cache_write.saturating_add(msg.tokens.cache_write);
        entry.reasoning = entry.reasoning.saturating_add(msg.tokens.reasoning);
        entry.message_count = entry.message_count.saturating_add(msg.message_count.max(0));
        entry.cost += msg.cost;
    }

    let mut entries: Vec<MonthlyUsageV2> = month_map
        .into_iter()
        .map(|(month, agg)| MonthlyUsageV2 {
            month,
            models: agg.models.into_iter().collect(),
            input: agg.input,
            output: agg.output,
            cache_read: agg.cache_read,
            cache_write: agg.cache_write,
            reasoning: agg.reasoning,
            message_count: agg.message_count,
            cost: agg.cost,
        })
        .collect();

    entries.sort_by(|a, b| a.month.cmp(&b.month));
    entries
}

pub async fn get_monthly_report_v2(options: ReportOptions) -> Result<MonthlyReportV2, String> {
    let start = Instant::now();

    let home_dir = get_home_dir_string(&options.home_dir)?;

    let clients: Vec<String> = options.clients.clone().unwrap_or_else(|| {
        let mut clients: Vec<String> = ClientId::ALL
            .iter()
            .map(|c| c.as_str().to_string())
            .collect();
        clients.push("synthetic".to_string());
        clients
    });

    let pricing = load_pricing_for_local_parse().await;
    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        pricing.as_deref(),
        options.use_env_roots,
        &options.scanner_settings,
    );

    let filtered = filter_messages_for_report(all_messages, &options);
    let entries = aggregate_monthly_usage_v2_entries(filtered);

    // f64's Sum identity is -0.0, so an empty report would serialize as
    // "totalCost": -0.0; adding +0.0 normalizes the sign without changing
    // any non-zero total.
    let total_cost: f64 = entries.iter().map(|e| e.cost).sum::<f64>() + 0.0;

    Ok(MonthlyReportV2 {
        entries,
        total_cost,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

/// Generate the original monthly report shape.
///
/// New callers that need reasoning tokens should use [`get_monthly_report_v2`].
pub async fn get_monthly_report(options: ReportOptions) -> Result<MonthlyReport, String> {
    Ok(get_monthly_report_v2(options).await?.into_legacy())
}
