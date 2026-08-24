use gpui::Pixels;

use crate::{SortDirection, SortState, ToksApp};

use super::TableColumn;

pub(super) const PAGE_CONTENT_MAX_WIDTH: f32 = 1280.;
const PAGE_HORIZONTAL_PADDING: f32 = 48.;
const CARD_HORIZONTAL_PADDING: f32 = 32.;
const COLUMN_GAP: f32 = 8.;

#[derive(Clone, Copy)]
pub(super) struct TableLayout {
    inner_width: f32,
}

impl TableLayout {
    pub(super) fn from_detail_width(detail_width: Pixels) -> Self {
        let content_width = (detail_width.to_f64() as f32).min(PAGE_CONTENT_MAX_WIDTH);
        Self {
            inner_width: (content_width - PAGE_HORIZONTAL_PADDING - CARD_HORIZONTAL_PADDING)
                .max(0.),
        }
    }

    pub(super) fn columns<C: TableColumn>(self, active: Option<C::SortColumn>) -> Vec<C> {
        let mut columns = C::ALL.to_vec();
        for &removable in C::REMOVAL_ORDER {
            if required_width::<C>(&columns) <= self.inner_width {
                break;
            }
            if active == Some(removable.sort_column()) {
                continue;
            }
            columns.retain(|column| *column != removable);
        }
        columns
    }
}

fn required_width<C: TableColumn>(columns: &[C]) -> f32 {
    C::LABEL_WIDTH
        + columns.iter().copied().map(C::width).sum::<f32>()
        + COLUMN_GAP * columns.len() as f32
}

#[derive(Clone, Copy)]
pub(super) struct TableContext<'cx, 'app, C> {
    layout: TableLayout,
    sort: SortState<C>,
    cx: &'cx gpui::Context<'app, ToksApp>,
}

impl<'cx, 'app, C: Copy> TableContext<'cx, 'app, C> {
    pub(super) fn new(
        layout: TableLayout,
        sort: SortState<C>,
        cx: &'cx gpui::Context<'app, ToksApp>,
    ) -> Self {
        Self { layout, sort, cx }
    }

    pub(super) fn unsorted(layout: TableLayout, cx: &'cx gpui::Context<'app, ToksApp>) -> Self {
        Self::new(
            layout,
            SortState {
                column: None,
                direction: SortDirection::Descending,
            },
            cx,
        )
    }

    pub(super) fn columns<T: TableColumn<SortColumn = C>>(self) -> Vec<T> {
        self.layout.columns(self.sort.column)
    }

    pub(super) fn sort(self) -> SortState<C> {
        self.sort
    }

    pub(super) fn cx(self) -> &'cx gpui::Context<'app, ToksApp> {
        self.cx
    }
}
