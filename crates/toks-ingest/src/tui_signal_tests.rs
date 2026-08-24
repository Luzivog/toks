use serial_test::serial;

use super::tui_signal::{
    is_tui_active, route_stderr, take_deferred_stderr_for_test, transition_tui_active,
    TuiActiveGuard,
};

#[test]
#[serial]
fn defers_stderr_until_the_tui_is_inactive() {
    let _restore = TuiActiveGuard::capture();

    assert!(
        transition_tui_active(false).is_empty(),
        "the test must not inherit deferred diagnostics"
    );
    assert!(transition_tui_active(true).is_empty());
    assert!(is_tui_active());

    let marker = "deferred TUI diagnostic".to_string();
    assert!(route_stderr(marker.clone()).is_none());

    assert_eq!(transition_tui_active(false), vec![marker]);
    assert!(!is_tui_active());
    assert!(transition_tui_active(false).is_empty());
    assert_eq!(
        route_stderr("immediate diagnostic".to_string()),
        Some("immediate diagnostic".to_string())
    );
}

/// The guard's whole reason to exist, mirroring
/// `paths::test_env::EnvGuard`'s equivalent proof: a panic between
/// mutating `TUI_ACTIVE` and restoring it must not leak the mutation into
/// the next test scheduled in this binary.
#[test]
#[serial]
fn tui_active_guard_restores_even_when_the_probe_panics() {
    // Practise what the test preaches: restore on the way out however
    // this test exits.
    let _restore = TuiActiveGuard::capture();
    let _discarded = transition_tui_active(false);

    // The panic below is deliberate. It unwinds on this test's own thread,
    // which libtest already captures, so no process-global panic hook is
    // swapped to keep it out of the output.
    let outcome = std::panic::catch_unwind(|| {
        let mut tui = TuiActiveGuard::capture();
        tui.set(true);
        assert!(route_stderr("deferred by the probe".to_string()).is_none());
        panic!("simulated assertion failure");
    });

    assert!(outcome.is_err(), "the probe closure must have panicked");
    assert!(
        !is_tui_active(),
        "TuiActiveGuard must restore the previous value while unwinding"
    );
    assert!(
        take_deferred_stderr_for_test().is_empty(),
        "TuiActiveGuard must drain what the probe deferred instead of \
         leaving it for the next test"
    );
}

/// Restoring is not the same as tearing down. When the captured previous
/// value is active, an enclosing scope still owns the terminal, and the
/// diagnostics queued for its eventual restore must outlive this guard.
#[test]
#[serial]
fn tui_active_guard_restoring_an_active_scope_keeps_the_deferred_queue() {
    // The outermost guard is what cleans up: its own previous value is
    // inactive, so its drop drains the queue and clears TUI_ACTIVE however
    // this test exits.
    let mut outer = TuiActiveGuard::capture();
    assert!(
        take_deferred_stderr_for_test().is_empty(),
        "the test must not inherit deferred diagnostics"
    );
    outer.set(true);

    let marker = "deferred inside a nested capture".to_string();
    {
        let mut nested = TuiActiveGuard::capture();
        assert!(
            nested.previous,
            "the nested guard must capture the enclosing active state"
        );
        nested.set(true);
        assert!(route_stderr(marker.clone()).is_none());
    }

    assert!(
        is_tui_active(),
        "restoring an active previous value must leave the TUI active"
    );
    assert_eq!(
        take_deferred_stderr_for_test(),
        vec![marker],
        "restoring an active scope must not discard the diagnostics that \
         scope still owes its eventual terminal restore"
    );
}
