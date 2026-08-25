use toks_core::rotation::{RotationSettings, ThreadId, ThreadOverrideChange};

use super::io::apply_thread_override;

#[test]
fn model_change_clears_a_reasoning_override_the_model_does_not_advertise() {
    let thread = ThreadId::new("thread-42");
    let mut settings = RotationSettings::default();
    settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ReasoningEffort(Some("ultra".into())),
        )
        .unwrap();

    let changed = apply_thread_override(
        &mut settings,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.6".into())),
        Some(&["low".into(), "medium".into(), "high".into()]),
    )
    .unwrap();

    assert!(changed);
    let thread_override = settings.thread_override(&thread).unwrap();
    assert_eq!(thread_override.model(), Some("gpt-5.6"));
    assert_eq!(thread_override.reasoning_effort(), None);
}

#[test]
fn model_change_preserves_a_reasoning_override_the_model_advertises() {
    let thread = ThreadId::new("thread-42");
    let mut settings = RotationSettings::default();
    settings
        .set_thread_override(
            &thread,
            ThreadOverrideChange::ReasoningEffort(Some("high".into())),
        )
        .unwrap();

    apply_thread_override(
        &mut settings,
        &thread,
        ThreadOverrideChange::Model(Some("gpt-5.6".into())),
        Some(&["low".into(), "medium".into(), "high".into()]),
    )
    .unwrap();

    let thread_override = settings.thread_override(&thread).unwrap();
    assert_eq!(thread_override.model(), Some("gpt-5.6"));
    assert_eq!(thread_override.reasoning_effort(), Some("high"));
}
