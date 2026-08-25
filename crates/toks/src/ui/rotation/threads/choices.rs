use toks_core::codex_router::account_activation::SelectableModel;

#[derive(Clone)]
pub(super) struct Choice {
    pub(super) value: String,
    pub(super) label: String,
}

pub(super) fn models(catalogue: &[SelectableModel], observed: Option<&str>) -> Vec<Choice> {
    let mut choices = Vec::new();
    for model in catalogue {
        push(&mut choices, &model.slug, model.slug.clone());
    }
    if let Some(value) = observed {
        push(&mut choices, value, value.to_owned());
    }
    choices
}

pub(super) fn reasoning(
    catalogue: &[SelectableModel],
    effective_model: Option<&str>,
) -> Vec<Choice> {
    let advertised = effective_model
        .and_then(|slug| catalogue.iter().find(|model| model.slug == slug))
        .map(|model| model.reasoning_efforts.as_slice())
        .filter(|efforts| !efforts.is_empty());
    let values = advertised.map(<[String]>::to_vec).unwrap_or_else(|| {
        ["low", "medium", "high", "xhigh", "max"]
            .into_iter()
            .map(str::to_owned)
            .collect()
    });
    let mut choices = Vec::new();
    for value in &values {
        push(&mut choices, value, display_effort(value));
    }
    choices
}

pub(super) fn tiers() -> Vec<Choice> {
    vec![
        Choice {
            value: "default".into(),
            label: "Default".into(),
        },
        Choice {
            value: "priority".into(),
            label: "Priority".into(),
        },
    ]
}

fn push(choices: &mut Vec<Choice>, value: &str, label: String) {
    if !value.is_empty() && !choices.iter().any(|choice| choice.value == value) {
        choices.push(Choice {
            value: value.to_owned(),
            label,
        });
    }
}

pub(super) fn display_effort(value: &str) -> String {
    match value {
        "xhigh" => "XHigh".into(),
        value => capitalize(value),
    }
}

fn capitalize(value: &str) -> String {
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    first.to_uppercase().chain(characters).collect()
}
