// The imports the one test here needs live in its body: the test is
// `#[cfg(unix)]`, and at module scope they would be unused imports on
// Windows.
#[allow(unused_imports)]
use super::*;

/// Unix-only, because the fixture this test needs cannot be placed on
/// Windows without leaving the sandbox.
///
/// The test has to write a pricing file into `legacy_dirs_cache_dir()`,
/// which is `dirs::cache_dir()/tokscope`. `dirs::cache_dir()` reads
/// `XDG_CACHE_HOME` on Linux and `$HOME` on macOS, so the redirects below
/// move it into the temp dir; on Windows it is a `SHGetKnownFolderPath`
/// call that no environment variable reaches, so it stays at the real
/// `%LOCALAPPDATA%\tokscope\`. Writing there would drop a pricing file in
/// the actual profile of whatever machine ran the suite — the sandbox
/// escape #997 was about — and the assertion would be meaningless anyway,
/// since that directory may already hold a cache the canonical lookup
/// finds first, so the fallback under test would never run.
///
/// `#[cfg(unix)]` rather than a runtime `return`: the previous form was an
/// early return with no marker, so on Windows this reported as a passing
/// test that asserted nothing. The condition is not dynamic — it is the
/// platform — so the gate belongs where a reader can see it. The sandbox
/// check itself is kept below as an assertion, which fails loudly instead
/// of skipping if a Unix host ever resolves the legacy root outside the
/// temp dirs.
#[cfg(unix)]
#[test]
#[serial_test::serial]
fn load_falls_back_to_legacy_dirs_cache_path() {
    use crate::paths::test_env::EnvGuard;
    use tempfile::TempDir;

    let temp_home = TempDir::new().unwrap();
    let temp_xdg_cache = TempDir::new().unwrap();
    let mut env = EnvGuard::capture(&[
        "HOME",
        "XDG_CACHE_HOME",
        "XDG_CONFIG_HOME",
        "TOKSCOPE_CONFIG_DIR",
    ]);
    env.set("HOME", temp_home.path());
    env.set("XDG_CACHE_HOME", temp_xdg_cache.path());
    // Pin XDG_CONFIG_HOME so paths::get_cache_dir() stays inside
    // the sandboxed HOME on Linux CI runners that set this var
    // globally — without the pin, the canonical path resolves
    // outside the temp dir and the legacy fallback never gets
    // exercised because the binary never tries the right legacy
    // root either.
    env.set("XDG_CONFIG_HOME", temp_home.path().join(".config"));
    env.remove("TOKSCOPE_CONFIG_DIR");

    let legacy_path = crate::paths::legacy_dirs_cache_dir()
        .unwrap()
        .join("pricing-litellm.json");

    // Never write the fixture outside the sandbox: this file lands in a
    // real user profile if the redirects above did not take. Assert rather
    // than skip, so a host where they stop working reports a failure
    // instead of a silently empty test. See the note on this fn for why
    // Windows is excluded at compile time rather than caught here.
    assert!(
        legacy_path.starts_with(temp_home.path()) || legacy_path.starts_with(temp_xdg_cache.path()),
        "legacy cache root resolved outside the sandbox ({}); writing the \
         fixture there would touch a real profile",
        legacy_path.display()
    );

    fs::create_dir_all(legacy_path.parent().unwrap()).unwrap();
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    fs::write(
        &legacy_path,
        format!(r#"{{"timestamp":{now},"data":{{"ok":true}}}}"#),
    )
    .unwrap();

    let loaded: Option<serde_json::Value> = load_cache("pricing-litellm.json");
    assert_eq!(loaded.unwrap()["ok"], serde_json::json!(true));
}
