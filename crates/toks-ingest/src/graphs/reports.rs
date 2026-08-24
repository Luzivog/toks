use super::build::{generate_graph_with_loaded_pricing, GraphPricingRequirement};
use crate::{
    filter_messages_for_report, get_home_dir_string, load_pricing_for_local_parse,
    parse_all_messages_with_pricing_with_env_strategy, pricing, sessionize, ClientId, GraphResult,
    ReportOptions, TimeMetricsReport,
};
use std::time::Instant;

pub async fn get_time_metrics_report(options: ReportOptions) -> Result<TimeMetricsReport, String> {
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
        None,
        options.use_env_roots,
        &options.scanner_settings,
    );

    let filtered = filter_messages_for_report(all_messages, &options);

    let intervals = sessionize::sessionize(&filtered, sessionize::DEFAULT_IDLE_GAP_MS);
    let metrics = sessionize::compute_time_metrics(&intervals, sessionize::DEFAULT_IDLE_GAP_MS);

    Ok(TimeMetricsReport {
        metrics,
        processing_time_ms: start.elapsed().as_millis() as u32,
    })
}

pub async fn generate_graph(options: ReportOptions) -> Result<GraphResult, String> {
    let pricing = pricing::PricingService::get_or_init().await?;
    generate_graph_with_loaded_pricing(options, Some(&pricing), GraphPricingRequirement::Lenient)
        .await
}

pub async fn generate_submission_graph(options: ReportOptions) -> Result<GraphResult, String> {
    let pricing = pricing::PricingService::get_or_init().await?;
    generate_graph_with_loaded_pricing(options, Some(&pricing), GraphPricingRequirement::Submission)
        .await
}

pub async fn generate_local_graph_report(options: ReportOptions) -> Result<GraphResult, String> {
    let pricing = load_pricing_for_local_parse().await;
    generate_graph_with_loaded_pricing(
        options,
        pricing.as_deref(),
        GraphPricingRequirement::Lenient,
    )
    .await
}
