use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use tokscope_core::history::ModelUsage;

use crate::{ModelSortColumn, Page, SortState, TokscopeApp};

use super::{claude_accent, codex_accent, fmt_cost_full, fmt_tokens, sort_action};

pub(super) fn model_columns_header(
    page: Page,
    sort: SortState<ModelSortColumn>,
    cx: &mut gpui::Context<TokscopeApp>,
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
        ("Input", 60., ModelSortColumn::Input),
        ("Cache R", 62., ModelSortColumn::CacheRead),
        ("Cache W", 62., ModelSortColumn::CacheWrite),
        ("Output", 60., ModelSortColumn::Output),
        ("Reason.", 64., ModelSortColumn::Reasoning),
        ("Msgs", 54., ModelSortColumn::Messages),
        ("Turns", 44., ModelSortColumn::Turns),
        ("Total", 68., ModelSortColumn::Total),
        ("Est. cost", 82., ModelSortColumn::Cost),
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
        .child(number_cell(fmt_tokens(model.input), 60., false))
        .child(number_cell(fmt_tokens(model.cache_read), 62., false))
        .child(number_cell(fmt_tokens(model.cache_write), 62., false))
        .child(number_cell(fmt_tokens(model.output), 60., false))
        .child(number_cell(fmt_tokens(model.reasoning), 64., false))
        .child(number_cell(fmt_tokens(model.messages), 54., false))
        .child(number_cell(fmt_tokens(model.turns), 44., false))
        .child(number_cell(fmt_tokens(model.tokens), 68., true))
        .child(number_cell(fmt_cost_full(model.cost), 82., true))
}

fn model_sort_header(
    label: &'static str,
    width: f32,
    column: ModelSortColumn,
    page: Page,
    sort: SortState<ModelSortColumn>,
    cx: &mut gpui::Context<TokscopeApp>,
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
    }
}
