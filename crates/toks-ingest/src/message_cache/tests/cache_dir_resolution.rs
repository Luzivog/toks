use super::*;

#[test]
#[serial_test::serial]
fn cache_warning_is_deferred_once_while_the_tui_is_active() {
    const CONTEXT: &str = "test source cache warning deferral";
    let mut tui = crate::tui_signal::TuiActiveGuard::capture();
    assert!(
        crate::tui_signal::take_deferred_stderr_for_test().is_empty(),
        "the test must not inherit deferred diagnostics"
    );

    // Deliberately the real process-global set, so the production entry
    // point and its once-per-context bookkeeping stay covered. The
    // poisoning test below is the one that needs an isolated set.
    tui.set(true);
    let path = Path::new("cache-warning-test");
    let error = std::io::Error::other("simulated cache failure");
    warn_cache_failure_once(CONTEXT, path, &error);
    warn_cache_failure_once(CONTEXT, path, &error);

    assert_eq!(
        crate::tui_signal::take_deferred_stderr_for_test(),
        vec![format!(
            "toks: warning: {CONTEXT} ({}): {error}",
            path.display()
        )],
        "a repeated failure should leave one complete warning for terminal restore"
    );
}

#[test]
#[serial_test::serial]
fn cache_warning_survives_a_poisoned_once_only_set() {
    const CONTEXT: &str = "test source cache warning after poisoning";
    let mut tui = crate::tui_signal::TuiActiveGuard::capture();
    assert!(
        crate::tui_signal::take_deferred_stderr_for_test().is_empty(),
        "the test must not inherit deferred diagnostics"
    );

    // Poison a set scoped to this test rather than the process-global one:
    // poisoning cannot be undone, so poisoning the real set would make
    // every later test in this binary depend on the recovery under test.
    let warned: Mutex<HashSet<&'static str>> = Mutex::new(HashSet::new());

    // An unrelated panic while the once-only set is locked poisons the
    // mutex. The warning must still reach the user instead of being
    // silently swallowed by the poison.
    //
    // No panic hook is installed here. The hook is process-global, so
    // swapping it would suppress the diagnostics of whatever else runs in
    // parallel; this unwind happens on the test's own thread, which
    // libtest already captures, so the expected panic message is only
    // printed if this test fails.
    let poisoned = std::panic::catch_unwind(|| {
        let _guard = warned.lock().expect("set is not yet poisoned");
        panic!("unrelated panic while holding the once-only set");
    });
    assert!(poisoned.is_err(), "the helper panic must have unwound");
    assert!(
        warned.is_poisoned(),
        "the once-only set must be poisoned for this test to mean anything"
    );

    tui.set(true);
    let path = Path::new("cache-warning-poison-test");
    let error = std::io::Error::other("simulated cache failure");
    warn_cache_failure_once_in(&warned, CONTEXT, path, &error);
    warn_cache_failure_once_in(&warned, CONTEXT, path, &error);

    assert_eq!(
        crate::tui_signal::take_deferred_stderr_for_test(),
        vec![format!(
            "toks: warning: {CONTEXT} ({}): {error}",
            path.display()
        )],
        "a poisoned once-only set must still defer exactly one warning"
    );
}

fn restore_env_var(key: &str, value: Option<impl AsRef<std::ffi::OsStr>>) {
    unsafe {
        match value {
            Some(value) => std::env::set_var(key, value),
            None => std::env::remove_var(key),
        }
    }
}

#[test]
#[serial_test::serial]
fn test_fallback_cache_dir_prefers_runtime_dir() {
    let runtime_dir = TempDir::new().unwrap();
    let original_xdg_runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();
    restore_env_var("XDG_RUNTIME_DIR", Some(runtime_dir.path()));

    {
        assert_eq!(
            fallback_cache_dir(),
            Some(runtime_dir.path().join("tokscope"))
        );
    }

    restore_env_var("XDG_RUNTIME_DIR", original_xdg_runtime_dir);
}
