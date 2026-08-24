use super::model::aggregate_model_usage_entries;
use crate::{
    filter_messages_for_report, get_home_dir_string, load_pricing_for_local_parse,
    parse_all_messages_with_pricing_with_env_strategy, ClientId, ModelReport, ModelUsage,
    ReportOptions,
};
use std::time::Instant;

pub(crate) fn model_report_token_totals(entries: &[ModelUsage]) -> (i64, i64, i64, i64) {
    entries.iter().fold(
        (0, 0, 0, 0),
        |(input, output, cache_read, cache_write), entry| {
            (
                input.saturating_add(entry.input),
                output.saturating_add(entry.output),
                cache_read.saturating_add(entry.cache_read),
                cache_write.saturating_add(entry.cache_write),
            )
        },
    )
}

pub async fn get_model_report(options: ReportOptions) -> Result<ModelReport, String> {
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
    let entries = aggregate_model_usage_entries(filtered, &options.group_by);

    let (total_input, total_output, total_cache_read, total_cache_write) =
        model_report_token_totals(&entries);
    let total_messages: i32 = entries.iter().map(|e| e.message_count).sum();
    // f64's Sum identity is -0.0, so an empty report would serialize as
    // "totalCost": -0.0; adding +0.0 normalizes the sign without changing
    // any non-zero total.
    let total_cost: f64 = entries.iter().map(|e| e.cost).sum::<f64>() + 0.0;

    Ok(ModelReport {
        entries,
        total_input,
        total_output,
        total_cache_read,
        total_cache_write,
        total_messages,
        total_cost,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}
