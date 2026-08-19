use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use toks_core::history::ModelUsage;

use crate::{ModelSortColumn, Page, SortState, ToksApp};

use super::{claude_accent, codex_accent, fmt_cost_full, fmt_tokens, sort_action};

const INPUT_WIDTH: f32 = 72.;
const CACHE_READ_WIDTH: f32 = 88.;
const CACHE_WRITE_WIDTH: f32 = 90.;
const OUTPUT_WIDTH: f32 = 72.;
const REASONING_WIDTH: f32 = 82.;
const MESSAGES_WIDTH: f32 = 78.;
const TURNS_WIDTH: f32 = 56.;
const TOTAL_WIDTH: f32 = 78.;
const COST_WIDTH: f32 = 102.;

pub(super) fn model_columns_header(
    page: Page,
    sort: SortState<ModelSortColumn>,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    let mut header = h_flex()
        .gap_2()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex_1()
                .min_w(px(120.))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Model"),
        );
    for (label, width, column) in [
        ("Input", INPUT_WIDTH, ModelSortColumn::Input),
        ("Cache read", CACHE_READ_WIDTH, ModelSortColumn::CacheRead),
        (
            "Cache write",
            CACHE_WRITE_WIDTH,
            ModelSortColumn::CacheWrite,
        ),
        ("Output", OUTPUT_WIDTH, ModelSortColumn::Output),
        ("Reasoning", REASONING_WIDTH, ModelSortColumn::Reasoning),
        ("Messages", MESSAGES_WIDTH, ModelSortColumn::Messages),
        ("Turns", TURNS_WIDTH, ModelSortColumn::Turns),
        ("Total", TOTAL_WIDTH, ModelSortColumn::Total),
        ("Est. API cost", COST_WIDTH, ModelSortColumn::Cost),
    ] {
        header = header.child(model_sort_header(label, width, column, page, sort, cx));
    }
    header
}

pub(super) fn model_usage_row(model: &ModelUsage, page: Page, cx: &App) -> gpui::Div {
    let provider = model.provider.to_lowercase();
    let selector = format!("model-row-{}-{}-{}", page_id(page), provider, model.model);
    let color = if provider.contains("anthropic") || provider.contains("claude") {
        claude_accent()
    } else {
        codex_accent()
    };
    h_flex()
        .debug_selector(move || selector)
        .gap_2()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .flex_1()
                .min_w(px(120.))
                .gap_2()
                .items_center()
                .text_sm()
                .child(div().size_2().rounded_full().bg(color).flex_shrink_0())
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .font_medium()
                        .child(model.model.clone()),
                ),
        )
        .child(number_cell(fmt_tokens(model.input), INPUT_WIDTH, false))
        .child(number_cell(
            fmt_tokens(model.cache_read),
            CACHE_READ_WIDTH,
            false,
        ))
        .child(number_cell(
            fmt_tokens(model.cache_write),
            CACHE_WRITE_WIDTH,
            false,
        ))
        .child(number_cell(fmt_tokens(model.output), OUTPUT_WIDTH, false))
        .child(number_cell(
            fmt_tokens(model.reasoning),
            REASONING_WIDTH,
            false,
        ))
        .child(number_cell(
            fmt_tokens(model.messages),
            MESSAGES_WIDTH,
            false,
        ))
        .child(number_cell(fmt_tokens(model.turns), TURNS_WIDTH, false))
        .child(number_cell(fmt_tokens(model.tokens), TOTAL_WIDTH, true))
        .child(number_cell(fmt_cost_full(model.cost), COST_WIDTH, true))
}

fn model_sort_header(
    label: &'static str,
    width: f32,
    column: ModelSortColumn,
    page: Page,
    sort: SortState<ModelSortColumn>,
    cx: &mut gpui::Context<ToksApp>,
) -> Button {
    let active = sort.column == Some(column);
    sort_action(
        SharedString::from(format!(
            "model-sort-{}-{}",
            page_id(page),
            model_sort_column_id(column)
        )),
        label,
        width,
        active,
        sort.direction,
        cx,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        app.model_tables.toggle_sort(page, column);
        cx.notify();
    }))
}

fn model_sort_column_id(column: ModelSortColumn) -> &'static str {
    match column {
        ModelSortColumn::Input => "input",
        ModelSortColumn::CacheRead => "cache-read",
        ModelSortColumn::CacheWrite => "cache-write",
        ModelSortColumn::Output => "output",
        ModelSortColumn::Reasoning => "reasoning",
        ModelSortColumn::Messages => "messages",
        ModelSortColumn::Turns => "turns",
        ModelSortColumn::Total => "total",
        ModelSortColumn::Cost => "cost",
    }
}

fn number_cell(value: String, width: f32, strong: bool) -> gpui::Div {
    div()
        .w(px(width))
        .flex_shrink_0()
        .text_right()
        .text_xs()
        .when(strong, |cell| cell.font_semibold())
        .child(value)
}

fn page_id(page: Page) -> &'static str {
    match page {
        Page::Overview => "overview",
        Page::Hourly => "hourly",
        Page::Daily => "daily",
        Page::Monthly => "monthly",
        Page::AllTime => "all-time",
    }
}
