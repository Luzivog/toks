use toks_core::history::UsageBucket;

use crate::UsageSortColumn;

use super::{fmt_cost_full, fmt_cost_per_million, fmt_tokens, TableColumn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum UsageColumn {
    Turns,
    Messages,
    Input,
    Output,
    Reasoning,
    CacheRead,
    CacheWrite,
    CostPerMillion,
    Total,
    Cost,
}

impl UsageColumn {
    pub(super) const ALL: [Self; 10] = [
        Self::Turns,
        Self::Messages,
        Self::Input,
        Self::Output,
        Self::Reasoning,
        Self::CacheRead,
        Self::CacheWrite,
        Self::CostPerMillion,
        Self::Total,
        Self::Cost,
    ];
}

impl TableColumn for UsageColumn {
    type Row = UsageBucket;
    type SortColumn = UsageSortColumn;

    const ALL: &'static [Self] = &Self::ALL;
    const LABEL_WIDTH: f32 = 130.;
    const REMOVAL_ORDER: &'static [Self] = &[
        Self::Turns,
        Self::Messages,
        Self::Reasoning,
        Self::CacheWrite,
        Self::Output,
        Self::CacheRead,
        Self::Input,
        Self::CostPerMillion,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Turns => "Turns",
            Self::Messages => "Messages",
            Self::Input => "Input",
            Self::Output => "Output",
            Self::Reasoning => "Reasoning",
            Self::CacheRead => "Cache read",
            Self::CacheWrite => "Cache write",
            Self::CostPerMillion => "Avg. $ / 1M",
            Self::Total => "Total",
            Self::Cost => "Est. API cost",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Turns => "turns",
            Self::Messages => "messages",
            Self::Input => "input",
            Self::Output => "output",
            Self::Reasoning => "reasoning",
            Self::CacheRead => "cache-read",
            Self::CacheWrite => "cache-write",
            Self::CostPerMillion => "cost-per-million",
            Self::Total => "total",
            Self::Cost => "cost",
        }
    }

    fn width(self) -> f32 {
        match self {
            Self::Turns => 58.,
            Self::Messages => 72.,
            Self::Input | Self::Output | Self::Reasoning => 82.,
            Self::CacheRead | Self::CacheWrite | Self::Total => 88.,
            Self::CostPerMillion => 92.,
            Self::Cost => 98.,
        }
    }

    fn sort_column(self) -> UsageSortColumn {
        match self {
            Self::Turns => UsageSortColumn::Turns,
            Self::Messages => UsageSortColumn::Messages,
            Self::Input => UsageSortColumn::Input,
            Self::Output => UsageSortColumn::Output,
            Self::Reasoning => UsageSortColumn::Reasoning,
            Self::CacheRead => UsageSortColumn::CacheRead,
            Self::CacheWrite => UsageSortColumn::CacheWrite,
            Self::CostPerMillion => UsageSortColumn::CostPerMillion,
            Self::Total => UsageSortColumn::Total,
            Self::Cost => UsageSortColumn::Cost,
        }
    }

    fn value(self, bucket: &UsageBucket) -> String {
        match self {
            Self::Turns => fmt_tokens(bucket.turns),
            Self::Messages => fmt_tokens(bucket.messages),
            Self::Input => fmt_tokens(bucket.input),
            Self::Output => fmt_tokens(bucket.output),
            Self::Reasoning => fmt_tokens(bucket.reasoning),
            Self::CacheRead => fmt_tokens(bucket.cache_read),
            Self::CacheWrite => fmt_tokens(bucket.cache_write),
            Self::CostPerMillion => fmt_cost_per_million(bucket.cost, bucket.tokens),
            Self::Total => fmt_tokens(bucket.tokens),
            Self::Cost => fmt_cost_full(bucket.cost),
        }
    }

    fn emphasized(self) -> bool {
        matches!(self, Self::Total | Self::Cost)
    }
}

#[cfg(test)]
mod tests {
    use super::{TableColumn, UsageColumn};

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
}
