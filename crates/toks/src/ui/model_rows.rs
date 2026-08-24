use gpui::{div, prelude::*, px, SharedString};
use gpui_component::{button::Button, h_flex, ActiveTheme, StyledExt};
use toks_core::history::ModelUsage;

use crate::{ModelSortColumn, Page};

use super::{
    accent_for_model_provider, table_cell, table_sort_header, ModelColumn, TableColumn,
    TableContext,
};

pub(super) fn model_columns_header(
    page: Page,
    table: TableContext<'_, '_, ModelSortColumn>,
) -> gpui::Div {
    let cx = table.cx();
    let mut header = h_flex()
        .gap_2()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex_1()
                .min_w(px(ModelColumn::LABEL_WIDTH))
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Model"),
        );
    for column in table.columns::<ModelColumn>() {
        header = header.child(model_sort_header(column, page, table));
    }
    header
}

pub(super) fn model_usage_row(
    model: &ModelUsage,
    page: Page,
    table: TableContext<'_, '_, ModelSortColumn>,
) -> gpui::Div {
    let cx = table.cx();
    let provider = model.provider.to_lowercase();
    let selector = format!("model-row-{}-{}-{}", page.slug(), provider, model.model);
    let color = accent_for_model_provider(&model.provider);
    let mut row = h_flex()
        .debug_selector(move || selector)
        .gap_2()
        .py_2()
        .border_t_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .flex_1()
                .min_w(px(ModelColumn::LABEL_WIDTH))
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
    for column in table.columns::<ModelColumn>() {
        row = row.child(number_cell(column, column.value(model)));
    }
    row
}

fn model_sort_header(
    column: ModelColumn,
    page: Page,
    table: TableContext<'_, '_, ModelSortColumn>,
) -> Button {
    let sort_column = column.sort_column();
    table_sort_header(
        SharedString::from(format!("model-sort-{}-{}", page.slug(), column.id())),
        column,
        table.sort(),
        table.cx(),
    )
    .on_click(table.cx().listener(move |app, _, _, cx| {
        app.model_tables.toggle_sort(page, sort_column);
        cx.notify();
    }))
}

fn number_cell(column: ModelColumn, value: String) -> gpui::Div {
    table_cell(column)
        .text_xs()
        .when(column.emphasized(), |cell| cell.font_semibold())
        .child(value)
}
