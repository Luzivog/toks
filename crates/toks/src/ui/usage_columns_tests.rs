use super::{table_column::TableColumn, usage_columns::UsageColumn};

#[test]
fn usage_columns_keep_the_shared_display_order() {
    assert_eq!(
        UsageColumn::ALL.map(UsageColumn::label),
        [
            "Turns",
            "Messages",
            "Input",
            "Output",
            "Reasoning",
            "Cache read",
            "Cache write",
            "Avg. $ / 1M",
            "Total",
            "Est. API cost",
        ]
    );
}
