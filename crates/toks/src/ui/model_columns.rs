use toks_core::history::ModelUsage;

use crate::ModelSortColumn;

use super::{fmt_cost_full, fmt_tokens, TableColumn};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ModelColumn {
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

impl ModelColumn {
    pub(super) const ALL: [Self; 9] = [
        Self::Input,
        Self::CacheRead,
        Self::CacheWrite,
        Self::Output,
        Self::Reasoning,
        Self::Messages,
        Self::Turns,
        Self::Total,
        Self::Cost,
    ];
}

impl TableColumn for ModelColumn {
    type Row = ModelUsage;
    type SortColumn = ModelSortColumn;

    const ALL: &'static [Self] = &Self::ALL;
    const LABEL_WIDTH: f32 = 120.;
    const REMOVAL_ORDER: &'static [Self] = &[
        Self::Turns,
        Self::Messages,
        Self::Reasoning,
        Self::CacheWrite,
        Self::Output,
        Self::CacheRead,
        Self::Input,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Input => "Input",
            Self::CacheRead => "Cache read",
            Self::CacheWrite => "Cache write",
            Self::Output => "Output",
            Self::Reasoning => "Reasoning",
            Self::Messages => "Messages",
            Self::Turns => "Turns",
            Self::Total => "Total",
            Self::Cost => "Est. API cost",
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Input => "input",
            Self::CacheRead => "cache-read",
            Self::CacheWrite => "cache-write",
            Self::Output => "output",
            Self::Reasoning => "reasoning",
            Self::Messages => "messages",
            Self::Turns => "turns",
            Self::Total => "total",
            Self::Cost => "cost",
        }
    }

    fn width(self) -> f32 {
        match self {
            Self::Input | Self::Output => 72.,
            Self::CacheRead => 88.,
            Self::CacheWrite => 90.,
            Self::Reasoning => 82.,
            Self::Messages => 78.,
            Self::Turns => 56.,
            Self::Total => 78.,
            Self::Cost => 102.,
        }
    }

    fn sort_column(self) -> ModelSortColumn {
        match self {
            Self::Input => ModelSortColumn::Input,
            Self::CacheRead => ModelSortColumn::CacheRead,
            Self::CacheWrite => ModelSortColumn::CacheWrite,
            Self::Output => ModelSortColumn::Output,
            Self::Reasoning => ModelSortColumn::Reasoning,
            Self::Messages => ModelSortColumn::Messages,
            Self::Turns => ModelSortColumn::Turns,
            Self::Total => ModelSortColumn::Total,
            Self::Cost => ModelSortColumn::Cost,
        }
    }

    fn value(self, model: &ModelUsage) -> String {
        match self {
            Self::Input => fmt_tokens(model.input),
            Self::CacheRead => fmt_tokens(model.cache_read),
            Self::CacheWrite => fmt_tokens(model.cache_write),
            Self::Output => fmt_tokens(model.output),
            Self::Reasoning => fmt_tokens(model.reasoning),
            Self::Messages => fmt_tokens(model.messages),
            Self::Turns => fmt_tokens(model.turns),
            Self::Total => fmt_tokens(model.tokens),
            Self::Cost => fmt_cost_full(model.cost),
        }
    }

    fn emphasized(self) -> bool {
        matches!(self, Self::Total | Self::Cost)
    }
}
