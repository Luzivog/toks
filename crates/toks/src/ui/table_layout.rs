use gpui::Pixels;

use crate::{ModelSortColumn, UsageSortColumn};

use super::{ModelColumn, UsageColumn};

pub(super) const PAGE_CONTENT_MAX_WIDTH: f32 = 1280.;
const PAGE_HORIZONTAL_PADDING: f32 = 48.;
const CARD_HORIZONTAL_PADDING: f32 = 32.;
const COLUMN_GAP: f32 = 8.;
const USAGE_LABEL_WIDTH: f32 = 130.;
const MODEL_LABEL_WIDTH: f32 = 120.;

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

    pub(super) fn usage_columns(self, active: Option<UsageSortColumn>) -> Vec<UsageColumn> {
        let mut columns = UsageColumn::ALL.to_vec();
        for removable in [
            UsageColumn::Turns,
            UsageColumn::Messages,
            UsageColumn::Reasoning,
            UsageColumn::CacheWrite,
            UsageColumn::Output,
            UsageColumn::CacheRead,
            UsageColumn::Input,
            UsageColumn::CostPerMillion,
        ] {
            if required_width(USAGE_LABEL_WIDTH, &columns, UsageColumn::width) <= self.inner_width {
                break;
            }
            if active == Some(removable.sort_column()) {
                continue;
            }
            columns.retain(|column| *column != removable);
        }
        columns
    }

    pub(super) fn model_columns(self, active: Option<ModelSortColumn>) -> Vec<ModelColumn> {
        let mut columns = ModelColumn::ALL.to_vec();
        for removable in [
            ModelColumn::Turns,
            ModelColumn::Messages,
            ModelColumn::Reasoning,
            ModelColumn::CacheWrite,
            ModelColumn::Output,
            ModelColumn::CacheRead,
            ModelColumn::Input,
        ] {
            if required_width(MODEL_LABEL_WIDTH, &columns, ModelColumn::width) <= self.inner_width {
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

fn required_width<C: Copy>(label_width: f32, columns: &[C], width: fn(C) -> f32) -> f32 {
    label_width
        + columns.iter().copied().map(width).sum::<f32>()
        + COLUMN_GAP * columns.len() as f32
}

#[cfg(test)]
mod tests {
    use gpui::px;

    use super::TableLayout;
    use crate::ui::{ModelColumn, UsageColumn};
    use crate::{ModelSortColumn, UsageSortColumn};

    #[test]
    fn screenshot_width_removes_low_priority_usage_columns() {
        let columns = TableLayout::from_detail_width(px(1038.)).usage_columns(None);

        assert!(!columns.contains(&UsageColumn::Turns));
        assert!(!columns.contains(&UsageColumn::Messages));
        assert!(columns.contains(&UsageColumn::Input));
        assert!(columns.contains(&UsageColumn::CostPerMillion));
        assert!(columns.contains(&UsageColumn::Total));
        assert!(columns.contains(&UsageColumn::Cost));
    }

    #[test]
    fn active_sort_column_stays_visible() {
        let columns =
            TableLayout::from_detail_width(px(1038.)).usage_columns(Some(UsageSortColumn::Turns));

        assert!(columns.contains(&UsageColumn::Turns));
        assert!(!columns.contains(&UsageColumn::Messages));
        assert!(!columns.contains(&UsageColumn::Reasoning));
    }

    #[test]
    fn minimum_window_keeps_essential_usage_columns() {
        let columns = TableLayout::from_detail_width(px(940.)).usage_columns(None);

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
        let default = layout.model_columns(None);
        let sorted = layout.model_columns(Some(ModelSortColumn::Turns));

        assert!(!default.contains(&ModelColumn::Turns));
        assert!(sorted.contains(&ModelColumn::Turns));
        assert!(!sorted.contains(&ModelColumn::Messages));
        assert!(sorted.contains(&ModelColumn::Total));
        assert!(sorted.contains(&ModelColumn::Cost));
    }

    #[test]
    fn wide_layout_keeps_every_column() {
        let layout = TableLayout::from_detail_width(px(1600.));

        assert_eq!(layout.usage_columns(None), UsageColumn::ALL);
        assert_eq!(layout.model_columns(None), ModelColumn::ALL);
    }
}
