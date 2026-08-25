use super::ToksApp;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OverviewChartRange {
    LastTwentyFourHours,
    LastSevenDays,
    #[default]
    LastThirtyDays,
    AllTime,
}

impl OverviewChartRange {
    pub(crate) const ALL: [Self; 4] = [
        Self::LastTwentyFourHours,
        Self::LastSevenDays,
        Self::LastThirtyDays,
        Self::AllTime,
    ];

    pub(crate) const fn label(self) -> &'static str {
        match self {
            Self::LastTwentyFourHours => "Last 24 hours",
            Self::LastSevenDays => "Last 7 days",
            Self::LastThirtyDays => "Last 30 days",
            Self::AllTime => "All time",
        }
    }

    pub(crate) const fn title(self) -> &'static str {
        match self {
            Self::LastTwentyFourHours => "Usage — last 24 hours",
            Self::LastSevenDays => "Usage — last 7 days",
            Self::LastThirtyDays => "Usage — last 30 days",
            Self::AllTime => "Usage — all time",
        }
    }

    pub(crate) const fn slug(self) -> &'static str {
        match self {
            Self::LastTwentyFourHours => "last-24-hours",
            Self::LastSevenDays => "last-7-days",
            Self::LastThirtyDays => "last-30-days",
            Self::AllTime => "all-time",
        }
    }

    pub(crate) const fn chart_id(self) -> &'static str {
        match self {
            Self::LastTwentyFourHours => "overview-last-24-hours-usage",
            Self::LastSevenDays => "overview-last-7-days-usage",
            Self::LastThirtyDays => "overview-usage",
            Self::AllTime => "overview-all-time-usage",
        }
    }
}

impl ToksApp {
    pub(crate) fn set_overview_chart_range(&mut self, range: OverviewChartRange) {
        self.overview_chart_range = range;
    }
}
