use gpui::px;

use super::table_layout::TableLayout;
use crate::ui::{ModelColumn, UsageColumn};
use crate::{ModelSortColumn, UsageSortColumn};

#[test]
fn screenshot_width_removes_low_priority_usage_columns() {
    let columns = TableLayout::from_detail_width(px(1038.)).columns::<UsageColumn>(None);

    assert!(!columns.contains(&UsageColumn::Turns));
    assert!(!columns.contains(&UsageColumn::Messages));
    assert!(columns.contains(&UsageColumn::Input));
    assert!(columns.contains(&UsageColumn::CostPerMillion));
    assert!(columns.contains(&UsageColumn::Total));
    assert!(columns.contains(&UsageColumn::Cost));
}

#[test]
fn active_sort_column_stays_visible() {
    let columns = TableLayout::from_detail_width(px(1038.))
        .columns::<UsageColumn>(Some(UsageSortColumn::Turns));

    assert!(columns.contains(&UsageColumn::Turns));
    assert!(!columns.contains(&UsageColumn::Messages));
    assert!(!columns.contains(&UsageColumn::Reasoning));
}

#[test]
fn minimum_window_keeps_essential_usage_columns() {
    let columns = TableLayout::from_detail_width(px(940.)).columns::<UsageColumn>(None);

    assert!(!columns.contains(&UsageColumn::Turns));
    assert!(!columns.contains(&UsageColumn::Messages));
    assert!(!columns.contains(&UsageColumn::Reasoning));
    assert!(columns.contains(&UsageColumn::CacheWrite));
    assert!(columns.contains(&UsageColumn::CostPerMillion));
    assert!(columns.contains(&UsageColumn::Total));
    assert!(columns.contains(&UsageColumn::Cost));
}

#[test]
fn model_layout_uses_its_own_widths_and_active_sort() {
    let layout = TableLayout::from_detail_width(px(940.));
    let default = layout.columns::<ModelColumn>(None);
    let sorted = layout.columns::<ModelColumn>(Some(ModelSortColumn::Turns));

    assert!(!default.contains(&ModelColumn::Turns));
    assert!(sorted.contains(&ModelColumn::Turns));
    assert!(!sorted.contains(&ModelColumn::Messages));
    assert!(sorted.contains(&ModelColumn::Total));
    assert!(sorted.contains(&ModelColumn::Cost));
}

#[test]
fn wide_layout_keeps_every_column() {
    let layout = TableLayout::from_detail_width(px(1600.));

    assert_eq!(layout.columns::<UsageColumn>(None), UsageColumn::ALL);
    assert_eq!(layout.columns::<ModelColumn>(None), ModelColumn::ALL);
}
