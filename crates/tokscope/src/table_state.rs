use tokscope_core::history::UsagePeriod;

use crate::Page;

pub(crate) const DEFAULT_USAGE_ROWS: usize = 10;
pub(crate) const USAGE_PAGE_SIZE: usize = 50;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    fn reversed(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SortState<C> {
    pub column: Option<C>,
    pub direction: SortDirection,
}

impl<C: Copy + PartialEq> SortState<C> {
    const fn chronological() -> Self {
        Self {
            column: None,
            direction: SortDirection::Descending,
        }
    }

    const fn descending(column: C) -> Self {
        Self {
            column: Some(column),
            direction: SortDirection::Descending,
        }
    }

    fn toggle(&mut self, column: C) {
        if self.column == Some(column) {
            self.direction = self.direction.reversed();
        } else {
            self.column = Some(column);
            self.direction = SortDirection::Descending;
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UsageSortColumn {
    Period,
    Turns,
    Messages,
    Input,
    Output,
    CacheRead,
    Total,
    Cost,
}

pub(crate) struct UsageTablesState {
    sorts: [SortState<UsageSortColumn>; 3],
    visible_rows: [usize; 3],
}

impl UsageTablesState {
    pub(crate) fn new() -> Self {
        Self {
            sorts: [
                SortState::chronological(),
                SortState::descending(UsageSortColumn::Cost),
                SortState::descending(UsageSortColumn::Cost),
            ],
            visible_rows: [DEFAULT_USAGE_ROWS; 3],
        }
    }

    pub(crate) fn sort(&self, period: UsagePeriod) -> SortState<UsageSortColumn> {
        self.sorts[usage_period_index(period)]
    }

    pub(crate) fn toggle_sort(&mut self, period: UsagePeriod, column: UsageSortColumn) {
        self.sorts[usage_period_index(period)].toggle(column);
    }

    pub(crate) fn visible_limit(&self, period: UsagePeriod) -> usize {
        self.visible_rows[usage_period_index(period)]
    }

    pub(crate) fn show_more(&mut self, period: UsagePeriod) {
        let visible = &mut self.visible_rows[usage_period_index(period)];
        *visible = visible.saturating_add(USAGE_PAGE_SIZE);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ModelSortColumn {
    Input,
    CacheRead,
    CacheWrite,
    Output,
    Reasoning,
    Messages,
    Turns,
    Total,
    Cost,
}

pub(crate) struct ModelTablesState {
    sorts: [SortState<ModelSortColumn>; 4],
}

impl ModelTablesState {
    pub(crate) fn new() -> Self {
        Self {
            sorts: [SortState::descending(ModelSortColumn::Cost); 4],
        }
    }

    pub(crate) fn sort(&self, page: Page) -> SortState<ModelSortColumn> {
        self.sorts[page_index(page)]
    }

    pub(crate) fn toggle_sort(&mut self, page: Page, column: ModelSortColumn) {
        self.sorts[page_index(page)].toggle(column);
    }
}

const fn usage_period_index(period: UsagePeriod) -> usize {
    match period {
        UsagePeriod::Hourly => 0,
        UsagePeriod::Daily => 1,
        UsagePeriod::Monthly => 2,
    }
}

const fn page_index(page: Page) -> usize {
    match page {
        Page::Overview => 0,
        Page::Hourly => 1,
        Page::Daily => 2,
        Page::Monthly => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SortDirection, UsageSortColumn, UsageTablesState, DEFAULT_USAGE_ROWS, USAGE_PAGE_SIZE,
    };
    use tokscope_core::history::UsagePeriod;

    #[test]
    fn every_usage_metric_toggles_descending_then_ascending() {
        for column in [
            UsageSortColumn::Period,
            UsageSortColumn::Turns,
            UsageSortColumn::Messages,
            UsageSortColumn::Input,
            UsageSortColumn::Output,
            UsageSortColumn::CacheRead,
            UsageSortColumn::Total,
            UsageSortColumn::Cost,
        ] {
            let mut state = UsageTablesState::new();
            state.toggle_sort(UsagePeriod::Hourly, column);
            assert_eq!(state.sort(UsagePeriod::Hourly).column, Some(column));
            assert_eq!(
                state.sort(UsagePeriod::Hourly).direction,
                SortDirection::Descending
            );
            state.toggle_sort(UsagePeriod::Hourly, column);
            assert_eq!(
                state.sort(UsagePeriod::Hourly).direction,
                SortDirection::Ascending
            );
        }
    }

    #[test]
    fn pagination_is_independent_and_advances_by_fifty() {
        let mut state = UsageTablesState::new();
        assert_eq!(state.visible_limit(UsagePeriod::Hourly), DEFAULT_USAGE_ROWS);
        state.show_more(UsagePeriod::Hourly);
        assert_eq!(
            state.visible_limit(UsagePeriod::Hourly),
            DEFAULT_USAGE_ROWS + USAGE_PAGE_SIZE
        );
        assert_eq!(state.visible_limit(UsagePeriod::Daily), DEFAULT_USAGE_ROWS);
    }
}
