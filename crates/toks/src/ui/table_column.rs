use gpui::{div, prelude::*, px, App, SharedString};
use gpui_component::button::Button;

use crate::SortState;

use super::sort_action;

pub(super) trait TableColumn: Copy + Eq + 'static {
    type Row;
    type SortColumn: Copy + Eq;

    const ALL: &'static [Self];
    const LABEL_WIDTH: f32;
    const REMOVAL_ORDER: &'static [Self];

    fn label(self) -> &'static str;
    fn id(self) -> &'static str;
    fn width(self) -> f32;
    fn sort_column(self) -> Self::SortColumn;
    fn value(self, row: &Self::Row) -> String;
    fn emphasized(self) -> bool;
}

pub(super) fn table_cell<C: TableColumn>(column: C) -> gpui::Div {
    div().w(px(column.width())).flex_shrink_0().text_right()
}

pub(super) fn table_sort_header<C: TableColumn>(
    id: impl Into<SharedString>,
    column: C,
    sort: SortState<C::SortColumn>,
    cx: &App,
) -> Button {
    let sort_column = column.sort_column();
    sort_action(
        id,
        column.label(),
        column.width(),
        sort.column == Some(sort_column),
        sort.direction,
        cx,
    )
}
