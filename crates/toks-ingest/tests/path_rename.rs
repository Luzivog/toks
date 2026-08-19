use std::ffi::OsString;

use serial_test::serial;
use toks_ingest::clients::PathRoot;

struct EnvGuard(Vec<(&'static str, Option<OsString>)>);

impl EnvGuard {
    fn capture(keys: &[&'static str]) -> Self {
        Self(
            keys.iter()
                .map(|key| (*key, std::env::var_os(key)))
                .collect(),
        )
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in self.0.drain(..) {
            unsafe {
                match value {
                    Some(value) => std::env::set_var(key, value),
                    None => std::env::remove_var(key),
                }
            }
        }
    }
}

#[test]
#[serial]
fn config_path_prefers_the_current_override() {
    let _guard = EnvGuard::capture(&["TOKS_CONFIG_DIR", "TOKSCOPE_CONFIG_DIR"]);
    unsafe {
        std::env::set_var("TOKS_CONFIG_DIR", "/tmp/current-config-root");
        std::env::set_var("TOKSCOPE_CONFIG_DIR", "/tmp/legacy-config-root");
    }
    assert_eq!(
        PathRoot::Config.resolve("/tmp/home"),
        "/tmp/current-config-root"
    );
}

#[test]
#[serial]
fn config_path_accepts_the_legacy_override() {
    let _guard = EnvGuard::capture(&["TOKS_CONFIG_DIR", "TOKSCOPE_CONFIG_DIR"]);
    unsafe {
        std::env::remove_var("TOKS_CONFIG_DIR");
        std::env::set_var("TOKSCOPE_CONFIG_DIR", "/tmp/legacy-config-root");
    }
    assert_eq!(
        PathRoot::Config.resolve("/tmp/home"),
        "/tmp/legacy-config-root"
    );
}
