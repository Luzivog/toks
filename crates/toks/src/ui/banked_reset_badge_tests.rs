use super::banked_reset_badge::banked_reset_label;
use toks_core::Provider;

#[test]
fn labels_positive_codex_resets_only() {
    assert_eq!(
        banked_reset_label(Provider::Codex, 1).as_deref(),
        Some("1 reset")
    );
    assert_eq!(
        banked_reset_label(Provider::Codex, 3).as_deref(),
        Some("3 resets")
    );
    assert_eq!(banked_reset_label(Provider::Codex, 0), None);
    assert_eq!(banked_reset_label(Provider::Claude, 3), None);
}
