// @keep: these look like model names and are not, which is the whole problem.
/// Model ids that name a ROUTER, not a model.
///
/// Cursor, Copilot Desktop, Copilot VS Code, Kiro and Workbuddy all emit a
/// bare `auto` when the product chose the model on the user's behalf
/// (`sessions/cursor.rs:356`, `copilot_desktop.rs:123`, `copilot_vscode.rs:110`,
/// `kiro.rs:1135`, `workbuddy.rs:127`); `agent_review` is a Cursor feature.
/// Nothing in the session log records which model actually served the
/// request, so any rate attached to these describes a different model.
///
/// Left to the normal chain, `auto` matches by model part against every
/// dataset key ending in `/auto` and — because ties break on shortest key —
/// elects `morph/auto` at $0.85/$1.55, an unrelated code-apply vendor. That
/// is real money billed from a coincidence of spelling (#1062).
///
/// BARE ids only. A qualified `morph/auto` is a genuine Morph model and still
/// resolves. `custom-pricing.json` is consulted before this, so a user who
/// knows their router's effective rate can still state it.
const ROUTING_LABELS: &[&str] = &["auto", "agent_review"];

pub(crate) fn is_routing_label(model_id: &str) -> bool {
    let lower = model_id.trim().to_lowercase();
    ROUTING_LABELS.contains(&lower.as_str())
}
