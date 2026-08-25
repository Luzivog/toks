use crate::app::sidebar_open_for_layout;
use crate::sidebar_motion::SidebarMotion;
use crate::{
    ModelSortColumn, ModelTablesState, Page, SortDirection, UsageSortColumn, UsageTablesState,
};
use std::time::{Duration, Instant};
use toks_core::history::UsagePeriod;

#[test]
fn usage_periods_map_to_their_pages() {
    for (period, page) in [
        (UsagePeriod::Hourly, Page::Hourly),
        (UsagePeriod::Daily, Page::Daily),
        (UsagePeriod::Monthly, Page::Monthly),
    ] {
        assert_eq!(Page::from(period), page);
        assert_eq!(page.usage_period(), Some(period));
    }
}

#[test]
fn page_metadata_preserves_navigation_order() {
    assert_eq!(
        Page::ALL.map(|page| (page.slug(), page.title())),
        [
            ("overview", "Overview"),
            ("hourly", "Hourly"),
            ("daily", "Daily"),
            ("monthly", "Monthly"),
            ("all-time", "All time"),
            ("rotation", "Rotation"),
            ("settings", "Settings"),
        ]
    );
}

#[test]
fn sidebar_closes_when_entering_compact_layout() {
    assert!(!sidebar_open_for_layout(true, Some(false), true));
}

#[test]
fn compact_overlay_stays_open_until_the_user_dismisses_it() {
    assert!(sidebar_open_for_layout(true, Some(true), true));
}

#[test]
fn sidebar_reopens_when_returning_to_wide_layout() {
    assert!(sidebar_open_for_layout(false, Some(true), false));
}

#[test]
fn sidebar_motion_reverses_without_jumping() {
    let start = Instant::now();
    let mut motion = SidebarMotion::new();
    let closed = motion.update(false, false, true, start);
    assert_eq!(closed.panel, 0.0);

    motion.update(true, false, false, start);
    let midway = motion.update(true, false, false, start + Duration::from_millis(80));
    let reversed = motion.update(false, false, false, start + Duration::from_millis(80));
    assert_eq!(midway.panel, reversed.panel);

    let finished = motion.update(false, false, false, start + Duration::from_millis(260));
    assert_eq!(finished.panel, 0.0);
    assert!(!finished.active);
}

#[test]
fn responsive_collapse_does_not_flash_the_compact_scrim() {
    let start = Instant::now();
    let mut motion = SidebarMotion::new();
    motion.update(true, false, true, start);
    let compact = motion.update(false, true, false, start);
    assert_eq!(compact.panel, 1.0);
    assert_eq!(compact.scrim, 0.0);
}

#[test]
fn usage_sorting_is_independent_per_page_and_toggles_direction() {
    let mut state = UsageTablesState::new();
    state.toggle_sort(UsagePeriod::Hourly, UsageSortColumn::Total);
    assert_eq!(
        state.sort(UsagePeriod::Hourly).direction,
        SortDirection::Descending
    );
    state.toggle_sort(UsagePeriod::Hourly, UsageSortColumn::Total);
    assert_eq!(
        state.sort(UsagePeriod::Hourly).direction,
        SortDirection::Ascending
    );
    assert_eq!(
        state.sort(UsagePeriod::Daily).column,
        Some(UsageSortColumn::Cost)
    );
    assert_eq!(
        state.sort(UsagePeriod::Monthly).column,
        Some(UsageSortColumn::Cost)
    );
}

#[test]
fn model_sorting_defaults_to_cost_and_is_independent_per_page() {
    let mut state = ModelTablesState::new();
    assert_eq!(
        state.sort(Page::Overview).column,
        Some(ModelSortColumn::Cost)
    );
    state.toggle_sort(Page::Hourly, ModelSortColumn::Input);
    assert_eq!(
        state.sort(Page::Hourly).column,
        Some(ModelSortColumn::Input)
    );
    assert_eq!(
        state.sort(Page::Overview).column,
        Some(ModelSortColumn::Cost)
    );
    assert_eq!(
        state.sort(Page::AllTime).column,
        Some(ModelSortColumn::Cost)
    );
    state.toggle_sort(Page::AllTime, ModelSortColumn::Messages);
    assert_eq!(
        state.sort(Page::AllTime).column,
        Some(ModelSortColumn::Messages)
    );
    assert_eq!(
        state.sort(Page::Hourly).column,
        Some(ModelSortColumn::Input)
    );
}
