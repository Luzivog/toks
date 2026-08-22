use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use toks_core::history::ModelUsage;

use crate::{ModelSortColumn, Page, SortState, ToksApp};

use super::{claude_accent, codex_accent, opencode_accent, sort_action, ModelColumn, TableLayout};

pub(super) fn model_columns_header(
    page: Page,
    sort: SortState<ModelSortColumn>,
    layout: TableLayout,
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
    for column in layout.model_columns(sort.column) {
        header = header.child(model_sort_header(column, page, sort, cx));
    }
    header
}

pub(super) fn model_usage_row(
    model: &ModelUsage,
    page: Page,
    layout: TableLayout,
    active_sort: Option<ModelSortColumn>,
    cx: &App,
) -> gpui::Div {
    let provider = model.provider.to_lowercase();
    let selector = format!("model-row-{}-{}-{}", page_id(page), provider, model.model);
    let color = if provider.contains("anthropic") || provider.contains("claude") {
        claude_accent()
    } else if provider.contains("opencode")
        || provider.contains("google")
        || provider.contains("gemini")
        || provider.contains("zen")
        || provider.contains("xai")
        || provider.contains("grok")
    {
        opencode_accent()
    } else {
        codex_accent()
    };
    let mut row = h_flex()
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
        );
    for column in layout.model_columns(active_sort) {
        row = row.child(number_cell(column, column.value(model)));
    }
    row
}

fn model_sort_header(
    column: ModelColumn,
    page: Page,
    sort: SortState<ModelSortColumn>,
    cx: &mut gpui::Context<ToksApp>,
) -> Button {
    let sort_column = column.sort_column();
    let active = sort.column == Some(sort_column);
    sort_action(
        SharedString::from(format!("model-sort-{}-{}", page_id(page), column.id())),
        column.label(),
        column.width(),
        active,
        sort.direction,
        cx,
    )
    .on_click(cx.listener(move |app, _, _, cx| {
        app.model_tables.toggle_sort(page, sort_column);
        cx.notify();
    }))
}

fn number_cell(column: ModelColumn, value: String) -> gpui::Div {
    div()
        .w(px(column.width()))
        .flex_shrink_0()
        .text_right()
        .text_xs()
        .when(column.emphasized(), |cell| cell.font_semibold())
        .child(value)
}

fn page_id(page: Page) -> &'static str {
    match page {
        Page::Overview => "overview",
        Page::Hourly => "hourly",
        Page::Daily => "daily",
        Page::Monthly => "monthly",
        Page::AllTime => "all-time",
        Page::Rotation => "rotation",
    }
}
