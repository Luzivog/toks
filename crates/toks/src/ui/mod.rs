//! Toks rendering, organized around the user-facing sections rather than
//! one monolithic view file.

use gpui::{App, Hsla, IntoElement};

use crate::{Page, ToksApp};

mod account_drag;
mod account_email;
mod account_menu;
mod action;
mod all_time;
mod all_time_data;
mod banked_reset_badge;
mod banked_reset_tooltip;
#[cfg(test)]
mod chart_data_tests;
mod chart_layout;
mod chart_plot;
mod chart_tooltip;
mod format;
mod history_error;
mod history_freshness;
mod limit_issue;
mod limit_rows;
mod limit_section;
mod limit_section_error;
mod limit_status;
mod loading_chart;
mod loading_content;
mod model_columns;
mod model_data;
mod model_rows;
mod models;
mod overview;
mod overview_metrics;
mod pages;
mod plan_badge;
mod quota_row;
mod rotation;
mod section;
mod sidebar;
mod summary;
mod table_layout;
mod theme;
mod usage_chart;
mod usage_columns;
mod usage_metric_row;
mod usage_points;
mod usage_range;
mod usage_rows;
mod usage_table;

use account_drag::account_drop_target;
use action::{action_button, sort_action, text_action};
#[cfg(test)]
use all_time_data::{all_time_points, all_time_summary};
use banked_reset_badge::banked_reset_badge;
use chart_layout::summary_chart_row;
use chart_plot::provider_usage_chart;
#[cfg(test)]
use chart_plot::{usage_chart_maximum, usage_hover_geometry, usage_marker_top};
#[cfg(test)]
use chart_tooltip::provider_rows;
use chart_tooltip::{usage_point_tooltip, ProviderPoint};
use format::{
    cost_per_million, fmt_age, fmt_as_of, fmt_cost_full, fmt_cost_per_million, fmt_exact_local,
    fmt_reset, fmt_tokens,
};
use history_error::history_error_card;
use history_freshness::history_freshness_text;
use limit_issue::limit_issue_row;
use limit_rows::account_limits_group;
use limit_section::account_limits_section;
use limit_section_error::account_error_rows;
use limit_status::{limit_header_status, pending_limit_row};
use loading_chart::{
    loading_plot, loading_status, loading_summary_sidebar, overview_history_loading,
};
use loading_content::{account_limits_loading_content, usage_page_loading};
use model_columns::ModelColumn;
#[cfg(test)]
use model_data::aggregate_model_usage;
use model_data::{current_usage_date, period_model_usage, sort_model_usage};
use model_rows::{model_columns_header, model_usage_row};
use models::model_breakdown_card;
#[cfg(test)]
use overview::overview_usage_points;
use overview::{legend_chip, usage_block};
use overview_metrics::overview_metrics_card;
use quota_row::quota_row;
#[cfg(test)]
use quota_row::split_limit_label;
use section::{section_meta, section_title};
use summary::{usage_summary_sidebar, UsageSummary};
use table_layout::{TableLayout, PAGE_CONTENT_MAX_WIDTH};
use theme::{accent_for_provider, claude_accent, codex_accent, gauge_color, opencode_accent};
use usage_chart::{usage_chart_card, usage_chart_identity};
use usage_columns::UsageColumn;
use usage_metric_row::{usage_data_row, usage_metric_row};
use usage_points::{provider_point, source_bucket_values, usage_chart_points};
use usage_range::{
    hourly_bucket_day, hourly_bucket_full_label, sort_usage_buckets, usage_bucket_is_current,
    usage_bucket_label, usage_period_label, usage_range_label, visible_usage_buckets,
};
use usage_rows::{hourly_day_separator, usage_columns_header, usage_static_columns_header};
use usage_table::usage_history_card;
#[cfg(test)]
use usage_table::visible_usage_row_count;

pub fn page_accent(page: Page, cx: &App) -> Hsla {
    theme::page_accent(page, cx)
}

pub fn sidebar(app: &ToksApp, cx: &mut gpui::Context<ToksApp>, overlay: bool) -> impl IntoElement {
    sidebar::sidebar(app, cx, overlay)
}

pub fn detail(
    app: &ToksApp,
    detail_width: gpui::Pixels,
    cx: &mut gpui::Context<ToksApp>,
) -> impl IntoElement {
    pages::detail(app, detail_width, cx)
}

#[cfg(test)]
mod tests;
