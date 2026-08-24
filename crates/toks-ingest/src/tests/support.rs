use super::*;
pub(super) fn parse_all_messages_with_pricing(
    home_dir: &str,
    clients: &[String],
    pricing: Option<&pricing::PricingService>,
) -> Vec<UnifiedMessage> {
    parse_all_messages_with_pricing_with_env_strategy(
        home_dir,
        clients,
        pricing,
        false,
        &scanner::ScannerSettings::default(),
    )
}

pub(super) fn home_guard() -> crate::paths::test_env::EnvGuard {
    crate::paths::test_env::EnvGuard::capture(&["HOME"])
}

/// Point the message-cache root at a scratch directory for as long as the
/// returned guard is alive.
///
/// Redirecting `HOME` is enough on Unix and does nothing on Windows:
/// `paths::get_config_dir` resolves the Windows root through
/// `dirs::config_dir()`, a known-folder lookup that reads no environment
/// variable at all. Every test in this module then shared one real
/// `%APPDATA%\tokscope\cache` and loaded back the shards its neighbours had
/// written, so the counts came out higher than the entries the test itself
/// inserted — and which neighbours had run first decided by how much.
///
/// `TOKSCOPE_CONFIG_DIR` is the override `paths.rs` documents for exactly
/// this ("CI sandbox, tests, isolated profile") and it is consulted first on
/// every platform. On Unix it names the directory the `HOME` redirect
/// already produced, so nothing moves there; it also pins the root against a
/// globally-set `XDG_CONFIG_HOME`, which a `HOME`-only redirect leaks past
/// on Linux runners.
///
/// That reach is exactly why the restore has to be a `Drop` guard rather
/// than a trailing call. The `HOME`-only redirect this replaced was inert
/// on Windows, so leaking it past a panicking assertion cost nothing there;
/// `TOKSCOPE_CONFIG_DIR` is consulted first on *every* platform, so a leaked
/// one points every later test in the binary at a `TempDir` that has already
/// been dropped — the cross-test contamination this redirect exists to
/// remove, reintroduced one layer down. `serial_test` does not help: it
/// prevents overlap, not inheritance.
#[must_use = "the redirect is undone as soon as the guard drops; bind it to a \
                  named variable that outlives the test body"]
pub(super) fn redirect_cache_home(home: &std::path::Path) -> crate::paths::test_env::EnvGuard {
    let mut env = crate::paths::test_env::EnvGuard::capture(&["HOME", "TOKSCOPE_CONFIG_DIR"]);
    point_cache_home(&mut env, home);
    env
}

/// Re-aim a live [`redirect_cache_home`] at a different scratch directory.
///
/// The tests that compare a warm cache against a cold one switch roots
/// mid-body, and one switches back again to assert on the first root. They
/// want a re-point, not a nested guard: the guard already holds the values
/// from before the *first* redirect, and restoring those once at scope exit
/// is the correct end state no matter how many times the root moved.
pub(super) fn point_cache_home(env: &mut crate::paths::test_env::EnvGuard, home: &std::path::Path) {
    env.set("HOME", home);
    env.set("TOKSCOPE_CONFIG_DIR", home.join(".config").join("tokscope"));
}

/// A client's scan root under `home`, spelled the way a scan will spell it.
///
/// `ClientDef::resolve_path` pushes each relative component with the
/// platform separator (#1048), so on Windows a discovered file reads
/// `C:\home\.claude\projects\demo\session.jsonl`. A fixture that builds the
/// same file with `Path::join` gets a mixed spelling (`C:\home\.claude/projects\...`)
/// — the same file, a different string.
///
/// That difference is invisible until a test seeds the message cache by
/// hand and expects the next scan to find it, because `CachedPath` keys on
/// the OS string as written: two spellings are two keys, so the seeded
/// entry is never read and the parse silently falls back to a cold parse.
/// Seeding under the spelling the scan produces is what these tests mean.
/// Whether the cache *ought* to fold the two spellings into one key is a
/// separate question about the product; nothing here depends on the answer.
pub(super) fn client_scan_root(home: &std::path::Path, client: ClientId) -> std::path::PathBuf {
    std::path::PathBuf::from(
        client
            .data()
            .resolve_path_with_env_strategy(&home.to_string_lossy(), false),
    )
}
