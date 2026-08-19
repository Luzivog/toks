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
        UsageSortColumn::Reasoning,
        UsageSortColumn::CacheRead,
        UsageSortColumn::CacheWrite,
        UsageSortColumn::Total,
        UsageSortColumn::Cost,
        UsageSortColumn::CostPerMillion,
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
