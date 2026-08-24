use super::exclusions::exclude_unpriced_submission_messages;
use super::validation::{require_trustworthy_exclusions, validate_priced_messages};
use crate::{
    aggregator, bucket_tz, filter_messages_for_report, get_home_dir_string,
    parse_all_messages_with_pricing_with_env_strategy, pricing, sessionize, ClientId, GraphResult,
    ReportOptions, UnifiedMessage,
};
use std::time::Instant;

#[derive(Clone, Copy)]
pub(crate) enum GraphPricingRequirement {
    Lenient,
    Submission,
}

pub(crate) async fn generate_graph_with_loaded_pricing(
    options: ReportOptions,
    pricing: Option<&pricing::PricingService>,
    pricing_requirement: GraphPricingRequirement,
) -> Result<GraphResult, String> {
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

    let all_messages = parse_all_messages_with_pricing_with_env_strategy(
        &home_dir,
        &clients,
        pricing,
        options.use_env_roots,
        &options.scanner_settings,
    );

    let filtered = filter_messages_for_report(all_messages, &options);

    let bucket_timezone =
        bucket_tz::BucketTimezone::from_scanner_settings(&options.scanner_settings);

    build_graph_from_messages(
        filtered,
        pricing,
        pricing_requirement,
        start,
        &bucket_timezone,
    )
}

pub(crate) fn build_graph_from_messages(
    filtered: Vec<UnifiedMessage>,
    pricing: Option<&pricing::PricingService>,
    pricing_requirement: GraphPricingRequirement,
    start: Instant,
    bucket_timezone: &bucket_tz::BucketTimezone,
) -> Result<GraphResult, String> {
    let (filtered, unpriced_submission_exclusions) = match pricing_requirement {
        GraphPricingRequirement::Lenient => (filtered, Vec::new()),
        GraphPricingRequirement::Submission => {
            let (submitted, exclusions) = exclude_unpriced_submission_messages(filtered, pricing);
            require_trustworthy_exclusions(pricing, &exclusions)?;
            validate_priced_messages(&submitted, pricing)?;
            (submitted, exclusions)
        }
    };

    let intervals = sessionize::sessionize(&filtered, sessionize::DEFAULT_IDLE_GAP_MS);
    let time_metrics =
        sessionize::compute_time_metrics(&intervals, sessionize::DEFAULT_IDLE_GAP_MS);

    // Keyed by the same zone the messages were rebucketed into. Active time is
    // joined onto contributions by date below, so a mismatch here silently
    // drops a day's active time rather than misplacing it.
    let daily_active_time = sessionize::compute_daily_active_time_in(&intervals, bucket_timezone);
    let contributions = aggregator::aggregate_by_date(filtered);

    let processing_time_ms = start.elapsed().as_millis() as u32;
    let mut result = aggregator::generate_graph_result(contributions, processing_time_ms);
    result.time_metrics = Some(time_metrics);
    result.unpriced_submission_exclusions = unpriced_submission_exclusions;

    for contribution in &mut result.contributions {
        if let Some(&ms) = daily_active_time.get(&contribution.date) {
            contribution.active_time_ms = Some(ms);
        }
    }

    Ok(result)
}
