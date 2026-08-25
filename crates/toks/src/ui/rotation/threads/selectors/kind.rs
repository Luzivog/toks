use toks_core::rotation::ThreadOverrideChange;

use super::super::choices;

#[derive(Clone, Copy)]
pub(super) enum SelectorKind {
    Model,
    Reasoning,
    Tier,
}

impl SelectorKind {
    pub(super) const fn slug(self) -> &'static str {
        match self {
            Self::Model => "model",
            Self::Reasoning => "reasoning",
            Self::Tier => "tier",
        }
    }

    pub(super) const fn label(self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Reasoning => "Reasoning",
            Self::Tier => "Tier",
        }
    }

    pub(super) const fn width(self) -> f32 {
        match self {
            Self::Model => 170.,
            Self::Reasoning => 145.,
            Self::Tier => 120.,
        }
    }

    pub(super) fn display(self, value: &str) -> String {
        match self {
            Self::Model => value.to_owned(),
            Self::Reasoning => choices::display_effort(value),
            Self::Tier if value == "default" => "Default".into(),
            Self::Tier if value == "priority" => "Priority".into(),
            Self::Tier => value.to_owned(),
        }
    }

    pub(super) fn change(self, value: Option<String>) -> ThreadOverrideChange {
        match self {
            Self::Model => ThreadOverrideChange::Model(value),
            Self::Reasoning => ThreadOverrideChange::ReasoningEffort(value),
            Self::Tier => ThreadOverrideChange::ServiceTier(value),
        }
    }
}
