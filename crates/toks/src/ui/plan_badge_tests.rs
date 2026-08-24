use super::plan_badge::plan_badge_label;
use toks_core::limits::PlanMultiplier;

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
