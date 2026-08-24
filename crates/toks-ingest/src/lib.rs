#![deny(clippy::all)]
pub mod accounting_delta;
mod aggregator;
pub mod bucket_tz;
mod cc_mirror;
pub mod clients;
pub mod content_extractor;
mod conversion;
mod devin_lookup;
mod dto;
pub mod fs_atomic;
mod graphs;
mod ingest;
mod message_cache;
pub mod model_alias;
mod model_name;
pub mod opencode_model_name;
mod parser;
pub mod paths;
pub mod pricing;
mod provider_identity;
mod report_request;
pub mod scanner;
pub mod sessionize;
pub mod sessions;
pub mod tui_signal;
#[cfg(test)]
mod tui_signal_tests;
mod usage_reports;
pub use aggregator::{
    aggregate_by_date, aggregate_by_session, calculate_intensities, calculate_summary,
    calculate_years, generate_graph_result,
};
pub use bucket_tz::BucketTimezone;
pub use clients::{ClientCounts, ClientDef, ClientId, PathRoot};
pub use conversion::parsed_to_unified;
#[cfg(test)]
pub(crate) use conversion::select_local_parse_pricing;
pub(crate) use conversion::{
    apply_pricing_if_available, filter_parsed_messages, filter_unified_messages,
    load_pricing_for_local_parse, should_keep_deduped_message, unified_to_parsed,
};
pub(crate) use devin_lookup::{devin_desktop_lookup_cell_for_snapshot, DevinDesktopLookupCache};
pub use dto::{
    ClientContribution, DailyContribution, DailyTotals, DataSummary, GraphMeta, GraphResult,
    GroupBy, HourlyReport, HourlyUsage, LocalParseOptions, ModelPerformance, ModelReport,
    ModelUsage, MonthlyReport, MonthlyReportV2, MonthlyUsage, MonthlyUsageV2, ParsedMessage,
    ParsedMessages, ReportOptions, SessionContribution, TimeMetricsReport, TokenBreakdown,
    UnpricedSubmissionExclusion, YearSummary,
};
pub(crate) use dto::{UNKNOWN_WORKSPACE_GROUP_KEY, UNKNOWN_WORKSPACE_LABEL};
#[cfg(test)]
pub(crate) use graphs::{
    build_graph_from_messages, generate_graph_with_loaded_pricing, is_generic_routing_label,
    validate_priced_messages, GraphPricingRequirement, INCOMPLETE_MODEL_PRICING_REASON,
    MISSING_MODEL_PRICING_REASON, ROUTING_LABEL_UNPRICED_REASON,
};
pub use graphs::{
    generate_graph, generate_local_graph_report, generate_submission_graph, get_time_metrics_report,
};
pub(crate) use ingest::{
    apply_headless_agent, dedupe_latest_trae_messages, is_headless_path, merge_workbuddy_messages,
    parse_all_messages_with_pricing_with_env_strategy, parse_hermes_sqlite_with_pricing,
    partition_workbuddy_paths, rebucket_days, retain_for_requested_clients,
};
pub use ingest::{
    parse_local_clients, parse_local_unified_messages, parse_local_unified_messages_with_pricing,
    parse_local_unified_messages_with_pricing_uncached,
};
pub use model_alias::ModelAliasMap;
pub use model_name::{canonical_model_id, model_name_for_grouping, normalize_model_for_grouping};
pub(crate) use model_name::{normalize_syntactic, strip_parenthesized_reasoning_tier};
pub use parser::{parse_json_file, parse_jsonl_file, ParseError};
pub(crate) use report_request::filter_messages_for_report;
pub use report_request::get_home_dir_string;
pub use scanner::{
    built_in_extra_scan_paths_for, copilot_exporter_path, copilot_exporter_path_with_env_strategy,
    devin_desktop_additional_roots, extra_scan_paths_for, headless_roots,
    headless_roots_with_env_strategy, parse_extra_dirs,
    prime_agent_session_roots_with_env_strategy, scan_all_clients,
    scan_all_clients_with_env_strategy, scan_all_clients_with_scanner_settings, scan_directory,
    CrushDbSource, ScanResult, ScannerSettings,
};
pub use sessionize::{
    compute_daily_active_time, compute_daily_active_time_in, compute_time_metrics, sessionize,
    SessionInterval, TimeMetrics, DEFAULT_IDLE_GAP_MS,
};
pub use sessions::{
    AccountingAlias, AccountingAliasScheme, CostSource, DurableIdentity, DurableIdentityScheme,
    IdentityStrength, UnifiedMessage,
};
#[cfg(test)]
pub(crate) use usage_reports::{
    aggregate_hourly_usage_entries, aggregate_model_usage_entries,
    aggregate_monthly_usage_v2_entries, model_report_token_totals, positive_token_total,
};
pub use usage_reports::{
    get_hourly_report, get_model_report, get_monthly_report, get_monthly_report_v2,
};

#[cfg(test)]
mod tests;
