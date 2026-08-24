use toks_core::limits::PlanMultiplier;

pub(super) fn plan_badge_label(plan: &str, multiplier: Option<PlanMultiplier>) -> String {
    let plan = match plan.to_ascii_lowercase().as_str() {
        // Codex calls its 5x product `prolite`; the customer-facing plan is Pro.
        "prolite" => "PRO".to_string(),
        _ => plan.to_uppercase(),
    };
    multiplier.map_or(plan.clone(), |multiplier| {
        format!("{plan} · {}×", multiplier.value())
    })
}
