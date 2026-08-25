use toks_core::history::UsagePeriod;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Overview,
    Hourly,
    Daily,
    Monthly,
    AllTime,
    Rotation,
    Settings,
}

impl Page {
    pub const ALL: [Self; 7] = [
        Self::Overview,
        Self::Hourly,
        Self::Daily,
        Self::Monthly,
        Self::AllTime,
        Self::Rotation,
        Self::Settings,
    ];

    pub fn slug(&self) -> &'static str {
        match self {
            Self::Overview => "overview",
            Self::Hourly => "hourly",
            Self::Daily => "daily",
            Self::Monthly => "monthly",
            Self::AllTime => "all-time",
            Self::Rotation => "rotation",
            Self::Settings => "settings",
        }
    }

    pub fn title(&self) -> &'static str {
        match self {
            Self::Overview => "Overview",
            Self::Hourly => "Hourly",
            Self::Daily => "Daily",
            Self::Monthly => "Monthly",
            Self::AllTime => "All time",
            Self::Rotation => "Rotation",
            Self::Settings => "Settings",
        }
    }

    pub fn usage_period(&self) -> Option<UsagePeriod> {
        match self {
            Self::Overview | Self::AllTime | Self::Rotation | Self::Settings => None,
            Self::Hourly => Some(UsagePeriod::Hourly),
            Self::Daily => Some(UsagePeriod::Daily),
            Self::Monthly => Some(UsagePeriod::Monthly),
        }
    }
}

impl From<UsagePeriod> for Page {
    fn from(period: UsagePeriod) -> Self {
        match period {
            UsagePeriod::Hourly => Self::Hourly,
            UsagePeriod::Daily => Self::Daily,
            UsagePeriod::Monthly => Self::Monthly,
        }
    }
}
