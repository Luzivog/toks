use tokscope_core::limits::PlanMultiplier;

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

#[cfg(test)]
mod tests {
    use super::plan_badge_label;
    use tokscope_core::limits::PlanMultiplier;

    #[test]
    fn formats_claude_and_codex_multiplier_badges() {
        assert_eq!(
            plan_badge_label("max", Some(PlanMultiplier::Twenty)),
            "MAX · 20×"
        );
        assert_eq!(
            plan_badge_label("prolite", Some(PlanMultiplier::Five)),
            "PRO · 5×"
        );
        assert_eq!(
            plan_badge_label("pro", Some(PlanMultiplier::Twenty)),
            "PRO · 20×"
        );
        assert_eq!(plan_badge_label("plus", None), "PLUS");
    }
}
