#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::history) enum RollupPeriod {
    All,
    Minute,
}

impl RollupPeriod {
    pub(super) fn from_storage(value: i64) -> rusqlite::Result<Self> {
        match value {
            0 => Ok(Self::All),
            1 => Ok(Self::Minute),
            _ => Err(rusqlite::Error::IntegralValueOutOfRange(0, value)),
        }
    }
}

#[derive(Clone, Debug)]
pub(in crate::history) struct ArchiveRollup {
    pub period: RollupPeriod,
    pub bucket_start_ms: i64,
    pub client: String,
    pub provider: String,
    pub model: String,
    pub cost_source: i64,
    pub long_context: bool,
    pub input: i64,
    pub output: i64,
    pub cache_read: i64,
    pub cache_write: i64,
    pub reasoning: i64,
    pub messages: i64,
    pub turns: i64,
    pub cost_nanos: i64,
    pub event_count: i64,
    pub pricing_basis: PricingBasis,
}

#[derive(Clone, Debug, Default)]
pub(in crate::history) struct ArchiveProjection {
    pub rollups: Vec<ArchiveRollup>,
    pub captured_since_ms: Option<i64>,
    pub captured_through_ms: Option<i64>,
    pub pending_sources: usize,
    pub projection_pending: usize,
    pub projection_complete: bool,
    pub strong_events: i64,
    pub weak_events: i64,
    pub conflicts: i64,
}
use toks_ingest::pricing::basis::PricingBasis;
