use super::test_env::EnvGuard;
use super::*;
use serial_test::serial;
use std::path::Path;

/// Every test in this module redirects at least one of these, and
/// `get_config_dir` reads all three, so capturing the set keeps a test
/// from leaking a partial redirect into the next one.
fn guard() -> EnvGuard {
    EnvGuard::capture(&[
        "TOKS_CONFIG_DIR",
        "TOKSCOPE_CONFIG_DIR",
        "TOKS_TEST_LEGACY_PROCESS_RUNNING",
        "HOME",
        "XDG_CONFIG_HOME",
    ])
}

/// The whole point of the guard over a trailing `restore_env(prev)` call:
/// a failing assertion panics *before* the manual restore runs, so the
/// redirect leaks into every later test in the process. `serial_test`
/// does not help — it prevents overlap, not inheritance. The next test to
/// run would then resolve `HOME` to a deleted `TempDir` and fail for a
/// reason unrelated to what it asserts, which is exactly the kind of
/// cascading, order-dependent failure that makes a Windows CI leg
/// unreadable.
#[test]
#[serial]
fn env_guard_restores_even_when_the_test_body_panics() {
    const SENTINEL: &str = "TOKSCOPE_ENV_GUARD_PANIC_PROBE";
    // Practise what the test preaches: if an assertion below fails, this
    // outer guard still restores the sentinel on the way out.
    let mut outer = EnvGuard::capture(&[SENTINEL]);
    outer.set(SENTINEL, "original");

    // The panic below is deliberate; keep it out of the test output.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let outcome = std::panic::catch_unwind(|| {
        let mut env = EnvGuard::capture(&[SENTINEL]);
        env.set(SENTINEL, "redirected");
        panic!("simulated assertion failure");
    });
    std::panic::set_hook(hook);

    assert!(outcome.is_err(), "the probe closure must have panicked");
    assert_eq!(
        std::env::var(SENTINEL).ok().as_deref(),
        Some("original"),
        "EnvGuard must restore the previous value while unwinding"
    );
}

/// The regression #997 is really about: this assertion has always held on
/// Unix and has never held on Windows, because `dirs::home_dir` consults
/// the Win32 profile API there and no environment variable can reach it.
/// Every home-rooted test in the workspace redirects `HOME` and assumes
/// the code under test follows, so this one assertion is what makes the
/// rest of them mean the same thing on both platforms.
#[test]
#[serial]
fn home_dir_follows_an_explicit_native_home_on_every_platform() {
    let mut env = guard();
    // `env::temp_dir()` is absolute and carries a drive prefix on Windows,
    // which is exactly the shape a `TempDir`-based test produces.
    let redirect = std::env::temp_dir().join("tokscope-core-home-dir-probe");
    env.set("HOME", &redirect);
    assert_eq!(home_dir(), Some(redirect));
}

/// MSYS2, Cygwin and Git Bash export `HOME=/home/<user>`. `Path` reads a
/// leading `/` on Windows as "root of the current drive", so honoring that
/// value would silently move a real user's credentials and scan roots to
/// `C:\home\<user>`. The absoluteness check in
/// `windows_native_home_override` exists solely to keep that from
/// happening.
#[test]
#[serial]
#[cfg(windows)]
fn home_dir_ignores_a_posix_shaped_home() {
    let mut env = guard();
    env.set("HOME", "/home/runner");
    assert_ne!(home_dir(), Some(PathBuf::from("/home/runner")));
}

/// `C:temp` carries a `Prefix` component but no root, so a prefix-only
/// check accepts it. Windows then resolves it against the *per-drive
/// current directory* for C: — the same `HOME` names a different directory
/// depending on where the process last `cd`-ed, so credentials and scan
/// roots move unpredictably. Only absolute native paths may redirect home.
///
/// Windows-only by construction: `C:temp` is a perfectly ordinary relative
/// filename on Unix, and `Path`'s prefix parsing only exists on Windows
/// targets, so there is no way to exercise this on macOS. It does run —
/// on the `windows-latest` leg this PR adds.
#[test]
#[serial]
#[cfg(windows)]
fn home_dir_ignores_a_drive_relative_home() {
    let mut env = guard();
    env.set("HOME", r"C:temp");
    assert_ne!(
        home_dir(),
        Some(PathBuf::from(r"C:temp")),
        "a drive-relative HOME resolves against the current directory on that drive"
    );
}

/// Same contract as `get_config_dir`: an exported-but-blank variable is a
/// misconfiguration, not a request to resolve every home-rooted path
/// against the process CWD.
#[test]
#[serial]
#[cfg(windows)]
fn home_dir_treats_an_empty_home_as_unset() {
    let mut env = guard();
    env.set("HOME", "");
    assert_ne!(home_dir(), Some(PathBuf::new()));
}

#[test]
#[serial]
fn env_override_is_returned_verbatim() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.set("TOKSCOPE_CONFIG_DIR", "/tmp/tokscope-custom");
    assert_eq!(get_config_dir(), PathBuf::from("/tmp/tokscope-custom"));
}

#[test]
#[serial]
fn renamed_override_wins_while_legacy_override_remains_supported() {
    let mut env = guard();
    env.set("TOKS_CONFIG_DIR", "/tmp/toks-custom");
    env.set("TOKSCOPE_CONFIG_DIR", "/tmp/tokscope-custom");
    assert_eq!(get_config_dir(), PathBuf::from("/tmp/toks-custom"));
}

#[test]
#[serial]
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn unix_default_is_dot_config_toks_under_home() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.remove("TOKSCOPE_CONFIG_DIR");
    env.remove("XDG_CONFIG_HOME");
    let home = tempfile::tempdir().unwrap();
    env.set("HOME", home.path());
    assert_eq!(get_config_dir(), home.path().join(".config/toks"),);
}

#[test]
#[serial]
#[cfg(target_os = "linux")]
fn linux_honors_xdg_config_home_when_set() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.remove("TOKSCOPE_CONFIG_DIR");
    let xdg = tempfile::tempdir().unwrap();
    env.set("XDG_CONFIG_HOME", xdg.path());
    assert_eq!(get_config_dir(), xdg.path().join("toks"),);
}

#[test]
#[serial]
fn cache_dir_is_cache_subdir_of_config_dir() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.set("TOKSCOPE_CONFIG_DIR", "/tmp/tokscope-cache-test");
    assert_eq!(
        get_cache_dir(),
        PathBuf::from("/tmp/tokscope-cache-test/cache")
    );
}

#[test]
#[serial]
fn legacy_helpers_return_none_when_overridden() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.set("TOKSCOPE_CONFIG_DIR", "/tmp/tokscope-override");
    assert!(legacy_dirs_cache_dir().is_none());
    assert!(legacy_dot_cache_tokscope_dir().is_none());
}

#[test]
#[serial]
fn legacy_helpers_return_some_when_not_overridden() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.remove("TOKSCOPE_CONFIG_DIR");
    assert!(
        legacy_dirs_cache_dir().is_some(),
        "dirs::cache_dir always resolves on test platforms"
    );
    assert!(
        legacy_dot_cache_tokscope_dir().is_some(),
        "HOME is set in test environments"
    );
}

#[test]
#[serial]
fn get_config_dir_treats_empty_override_as_unset() {
    // Empty TOKSCOPE_CONFIG_DIR previously slipped through and
    // produced PathBuf::from(""), which silently relocated cache
    // writes to ./cache and ./.tokscope. The resolver must agree
    // with `is_config_dir_overridden`: empty == unset.
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.set("TOKSCOPE_CONFIG_DIR", "");
    let resolved = get_config_dir();
    assert_ne!(
        resolved,
        PathBuf::from(""),
        "empty override must not resolve to the empty path"
    );
    assert!(
        resolved.is_absolute() || resolved == Path::new(".toks"),
        "empty override must fall through to platform default, got {resolved:?}"
    );
}

#[test]
#[serial]
fn is_config_dir_overridden_treats_empty_string_as_unset() {
    let mut env = guard();
    env.remove("TOKS_CONFIG_DIR");
    env.set("TOKSCOPE_CONFIG_DIR", "");
    assert!(!is_config_dir_overridden());
}
