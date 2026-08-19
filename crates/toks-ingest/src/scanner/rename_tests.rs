use super::{headless_roots, headless_roots_with_env_strategy};
use crate::paths::test_env::EnvGuard;
use serial_test::serial;
use std::path::PathBuf;

#[test]
#[serial]
fn headless_roots_use_toks_defaults() {
    let mut env = EnvGuard::capture(&["TOKS_HEADLESS_DIR", "TOKSCOPE_HEADLESS_DIR"]);
    env.remove("TOKS_HEADLESS_DIR");
    env.remove("TOKSCOPE_HEADLESS_DIR");

    assert_eq!(
        headless_roots("/tmp/toks-test-home"),
        vec![
            PathBuf::from("/tmp/toks-test-home/.config/toks/headless"),
            PathBuf::from("/tmp/toks-test-home/Library/Application Support/toks/headless"),
        ]
    );
}

#[test]
#[serial]
fn headless_roots_prefer_current_override() {
    let mut env = EnvGuard::capture(&["TOKS_HEADLESS_DIR", "TOKSCOPE_HEADLESS_DIR"]);
    env.set("TOKS_HEADLESS_DIR", "/current/headless");
    env.set("TOKSCOPE_HEADLESS_DIR", "/legacy/headless");

    assert_eq!(
        headless_roots("/tmp/home"),
        vec![PathBuf::from("/current/headless")]
    );
}

#[test]
#[serial]
fn headless_roots_accept_legacy_override() {
    let mut env = EnvGuard::capture(&["TOKS_HEADLESS_DIR", "TOKSCOPE_HEADLESS_DIR"]);
    env.remove("TOKS_HEADLESS_DIR");
    env.set("TOKSCOPE_HEADLESS_DIR", "/legacy/headless");

    assert_eq!(
        headless_roots("/tmp/home"),
        vec![PathBuf::from("/legacy/headless")]
    );
}

#[test]
#[serial]
fn headless_roots_ignore_overrides_when_disabled() {
    let mut env = EnvGuard::capture(&["TOKS_HEADLESS_DIR", "TOKSCOPE_HEADLESS_DIR"]);
    env.set("TOKS_HEADLESS_DIR", "/current/headless");
    env.set("TOKSCOPE_HEADLESS_DIR", "/legacy/headless");

    assert_eq!(
        headless_roots_with_env_strategy("/tmp/home", false),
        vec![
            PathBuf::from("/tmp/home/.config/toks/headless"),
            PathBuf::from("/tmp/home/Library/Application Support/toks/headless"),
        ]
    );
}
