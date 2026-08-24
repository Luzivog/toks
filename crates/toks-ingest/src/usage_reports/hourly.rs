use crate::{
    bucket_tz, filter_messages_for_report, get_home_dir_string, load_pricing_for_local_parse,
    model_name_for_grouping, parse_all_messages_with_pricing_with_env_strategy, ClientId,
    HourlyReport, HourlyUsage, ReportOptions, UnifiedMessage,
};
use std::collections::{HashMap, HashSet};
use std::time::Instant;

#[derive(Default)]
struct HourAggregator {
    clients: HashSet<String>,
    models: HashSet<String>,
    input: i64,
    output: i64,
    cache_read: i64,
    cache_write: i64,
    reasoning: i64,
    message_count: i32,
    turn_count: i32,
    cost: f64,
}

pub(crate) fn aggregate_hourly_usage_entries(
    messages: impl IntoIterator<Item = UnifiedMessage>,
    bucket_timezone: bucket_tz::BucketTimezone,
) -> Vec<HourlyUsage> {
    let mut hour_map: HashMap<String, HourAggregator> = HashMap::new();

    for msg in messages {
        let hour_key = if msg.timestamp > 0 {
            bucket_timezone
                .hour_key(msg.timestamp)
                .unwrap_or_else(|| format!("{} 00:00", msg.date))
        } else {
            format!("{} 00:00", msg.date)
        };

        let entry = hour_map.entry(hour_key).or_default();
        entry.clients.insert(msg.client.clone());
        entry.models.insert(model_name_for_grouping(
            &msg.client,
            &msg.provider_id,
            &msg.model_id,
        ));
        entry.input = entry.input.saturating_add(msg.tokens.input);
        entry.output = entry.output.saturating_add(msg.tokens.output);
        entry.cache_read = entry.cache_read.saturating_add(msg.tokens.cache_read);
        entry.cache_write = entry.cache_write.saturating_add(msg.tokens.cache_write);
        entry.reasoning = entry.reasoning.saturating_add(msg.tokens.reasoning);
        entry.message_count += msg.message_count.max(0);
        if msg.is_turn_start {
            entry.turn_count += 1;
        }
        entry.cost += msg.cost;
    }

    let mut entries: Vec<HourlyUsage> = hour_map
        .into_iter()
        .map(|(hour, agg)| HourlyUsage {
            hour,
            clients: {
                let mut clients: Vec<String> = agg.clients.into_iter().collect();
                clients.sort();
                clients
            },
            models: {
                let mut models: Vec<String> = agg.models.into_iter().collect();
                models.sort();
                models
            },
            input: agg.input,
            output: agg.output,
            cache_read: agg.cache_read,
            cache_write: agg.cache_write,
            message_count: agg.message_count,
            turn_count: agg.turn_count,
            reasoning: agg.reasoning,
            cost: agg.cost,
        })
        .collect();

    entries.sort_by(|a, b| a.hour.cmp(&b.hour));
    entries
}

/// Generate hourly usage report, keyed by "YYYY-MM-DD HH:00".
///
/// Derives the hour slot from `UnifiedMessage.timestamp` (Unix ms).
/// Falls back to date + "00:00" when timestamp is zero or missing.
pub async fn get_hourly_report(options: ReportOptions) -> Result<HourlyReport, String> {
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

    // The hour key embeds a date, and the timestamp-less fallback builds one
    // out of `msg.date`, which the rebucket pass already moved to the pinned
    // zone. Deriving it from the host would let one report disagree with
    // itself about which day an hour belongs to.
    let bucket_timezone =
        bucket_tz::BucketTimezone::from_scanner_settings(&options.scanner_settings);
    let entries = aggregate_hourly_usage_entries(filtered, bucket_timezone);

    // f64's Sum identity is -0.0, so an empty report would serialize as
    // "totalCost": -0.0; adding +0.0 normalizes the sign without changing
    // any non-zero total.
    let total_cost: f64 = entries.iter().map(|e| e.cost).sum::<f64>() + 0.0;

    Ok(HourlyReport {
        entries,
        total_cost,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}
