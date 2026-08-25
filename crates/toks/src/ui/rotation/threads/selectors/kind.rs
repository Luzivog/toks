use toks_core::rotation::ThreadOverrideChange;

use super::super::choices;

#[derive(Clone, Copy)]
pub(super) enum SelectorKind {
    Model,
    Reasoning,
    Tier,
}

impl SelectorKind {
    pub(super) const ALL: [Self; 3] = [Self::Model, Self::Reasoning, Self::Tier];

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
            Self::Model => 140.,
            Self::Reasoning | Self::Tier => 80.,
        }
    }

    pub(super) const fn value_width(self) -> f32 {
        self.width() - 32.
    }

    pub(super) const fn tooltip(self) -> &'static str {
        match self {
            Self::Model => "Choose a model override for this thread.",
            Self::Reasoning => "Choose a reasoning effort override for this thread.",
            Self::Tier => "Choose a service tier override for this thread.",
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
