mod path_resolution;
mod registration;

// These tests mutate process-global environment variables, so they take
// `#[serial]` rather than the private `Mutex` they used to share. The
// mutex made them exclusive with each other but not with the rest of the
// crate, which serializes on `serial_test` — two disjoint domains over one
// set of variables. `test_path_root_config_*` restores its snapshot of
// `TOKSCOPE_CONFIG_DIR` on the way out, which is a *clear* when the
// developer has none set, and that clear landed in the middle of the
// pricing cache tests that redirect the same variable: they went on
// asserting against a temp path the code was no longer writing to while
// the real `~/.config/tokscope/cache` took the write. One domain is the
// only arrangement that holds, so nothing here may reintroduce a second.
// The tests this module serializes restore through `EnvGuard` rather than a
// trailing restore call: a failing assertion panics before such a call
// runs, and `#[serial]` prevents overlap, not inheritance — so a redirect
// would leak into every later test in the process. `paths::tests` documents
// the same trap.

/// Join `relative` onto `root` with native separators throughout — the
/// behavior `ClientDef::resolve_path_with_env_strategy` must produce on
/// every platform (#1048). `Path::join` alone is not enough: it only
/// normalizes the junction, leaving the relative half's own `/`
/// separators untouched on Windows. Pushing each component is the only
/// spelling that yields `C:\Users\me\.codex\sessions` there.
fn native_join(root: &std::path::Path, relative: &str) -> String {
    let mut path = root.to_path_buf();
    for component in std::path::Path::new(relative).components() {
        path.push(component.as_os_str());
    }
    path.to_string_lossy().into_owned()
}

/// A home directory these tests can hand to the reasonix resolver.
///
/// Reasonix is the one client whose root runs through `Path` — tilde
/// expansion, an `is_absolute` check, and joins — rather than string
/// concatenation. `Path` on Windows reads a POSIX-shaped `/tmp/home` as
/// "the root of the current drive": not absolute, because it carries no
/// drive prefix, so the resolver's relative-path arm fires and prepends the
/// process's working directory. The other clients in this module keep
/// `/tmp/home` because they never look at the value.
fn reasonix_home() -> &'static str {
    if cfg!(windows) {
        "C:\\tmp\\home"
    } else {
        "/tmp/home"
    }
}

/// An absolute path on this platform, from `/`-separated components.
fn absolute_test_path(relative: &str) -> std::path::PathBuf {
    let root = if cfg!(windows) { "C:\\" } else { "/" };
    let mut path = std::path::PathBuf::from(root);
    for component in relative.split('/') {
        path.push(component);
    }
    path
}

/// `<root>/stats`, spelled the way [`super::ClientDef::resolve_path`] appends a
/// client's relative path: native separators throughout, on every
/// platform (#1048).
fn reasonix_stats_under(root: impl AsRef<std::path::Path>) -> String {
    native_join(root.as_ref(), "stats")
}

/// The reasonix root with no environment override.
///
/// Spelled out per platform rather than resolved, because the layout is the
/// claim: `~/.reasonix` on Unix, and `%HOME%\AppData\Roaming\reasonix` on
/// Windows, matching where the application actually keeps per-user config
/// there.
fn reasonix_default_root() -> std::path::PathBuf {
    let home = std::path::Path::new(reasonix_home());
    if cfg!(windows) {
        home.join("AppData").join("Roaming").join("reasonix")
    } else {
        home.join(".reasonix")
    }
}
