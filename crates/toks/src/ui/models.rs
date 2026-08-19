use gpui::{div, prelude::*};
use gpui_component::{h_flex, v_flex, ActiveTheme};
use toks_core::history::ModelUsage;

use crate::{ModelSortColumn, Page, SortState, ToksApp};

use super::{model_columns_header, model_usage_row, section_meta, section_title, sort_model_usage};

pub(super) fn model_breakdown_card(
    mut models: Vec<ModelUsage>,
    range: &'static str,
    page: Page,
    sort: SortState<ModelSortColumn>,
    cx: &mut gpui::Context<ToksApp>,
) -> gpui::Div {
    sort_model_usage(&mut models, sort);
    let mut card = v_flex()
        .p_4()
        .rounded_xl()
        .bg(cx.theme().secondary)
        .border_1()
        .border_color(cx.theme().border)
        .child(
            h_flex()
                .pb_3()
                .justify_between()
                .items_center()
                .child(section_title("Model breakdown"))
                .child(section_meta(range, cx)),
        );
    if models.is_empty() {
        return card.child(
            div()
                .py_4()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child("No model activity in this range."),
        );
    }

    card = card.child(model_columns_header(page, sort, cx));
    for model in &models {
        card = card.child(model_usage_row(model, page, cx));
    }
    card
}
